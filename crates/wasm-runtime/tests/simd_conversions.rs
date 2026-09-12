use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::ValidationError;

fn push_u32(bytes: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        bytes.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn push_i32_const(bytes: &mut Vec<u8>, mut value: i32) {
    bytes.push(0x41);
    loop {
        let byte = (value as u8) & 0x7f;
        value >>= 7;
        let sign = byte & 0x40 != 0;
        let done = (value == 0 && !sign) || (value == -1 && sign);
        bytes.push(if done { byte } else { byte | 0x80 });
        if done {
            break;
        }
    }
}

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn module(result_type: u8, instructions: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0, 0, 0];
    push_section(&mut bytes, 1, &[1, 0x60, 0, 1, result_type]);
    push_section(&mut bytes, 3, &[1, 0]);
    push_section(&mut bytes, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);
    let mut body = vec![0];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![1];
    push_u32(&mut code, body.len() as u32);
    code.extend(body);
    push_section(&mut bytes, 10, &code);
    bytes
}

fn simd(bytes: &mut Vec<u8>, subopcode: u32) {
    bytes.push(0xfd);
    push_u32(bytes, subopcode);
}

fn v128_const(bytes: &mut Vec<u8>, value: [u8; 16]) {
    simd(bytes, 12);
    bytes.extend_from_slice(&value);
}

fn run_v128(instructions: &[u8]) -> [u8; 16] {
    let parsed = parse_module(&module(0x7b, instructions)).expect("SIMD conversion fixture parses");
    let mut instance = Instance::new(parsed).expect("SIMD conversion fixture validates");
    match instance
        .invoke_export_values("run", &[])
        .expect("SIMD conversion fixture executes")
        .as_slice()
    {
        [Value::V128(value)] => **value,
        other => panic!("unexpected SIMD conversion result: {other:?}"),
    }
}

fn f32x4(values: [f32; 4]) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    for (lane, value) in values.into_iter().enumerate() {
        let start = lane * 4;
        bytes[start..start + 4].copy_from_slice(&value.to_bits().to_le_bytes());
    }
    bytes
}

fn f64x2(values: [f64; 2]) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    for (lane, value) in values.into_iter().enumerate() {
        let start = lane * 8;
        bytes[start..start + 8].copy_from_slice(&value.to_bits().to_le_bytes());
    }
    bytes
}

fn i32x4(values: [i32; 4]) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    for (lane, value) in values.into_iter().enumerate() {
        let start = lane * 4;
        bytes[start..start + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn u32x4(values: [u32; 4]) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    for (lane, value) in values.into_iter().enumerate() {
        let start = lane * 4;
        bytes[start..start + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn read_i32x4(bytes: [u8; 16]) -> [i32; 4] {
    std::array::from_fn(|lane| {
        let start = lane * 4;
        i32::from_le_bytes(bytes[start..start + 4].try_into().unwrap())
    })
}

fn read_u32x4(bytes: [u8; 16]) -> [u32; 4] {
    std::array::from_fn(|lane| {
        let start = lane * 4;
        u32::from_le_bytes(bytes[start..start + 4].try_into().unwrap())
    })
}

fn read_f32x4(bytes: [u8; 16]) -> [f32; 4] {
    std::array::from_fn(|lane| {
        let start = lane * 4;
        f32::from_bits(u32::from_le_bytes(
            bytes[start..start + 4].try_into().unwrap(),
        ))
    })
}

fn read_f64x2(bytes: [u8; 16]) -> [f64; 2] {
    std::array::from_fn(|lane| {
        let start = lane * 8;
        f64::from_bits(u64::from_le_bytes(
            bytes[start..start + 8].try_into().unwrap(),
        ))
    })
}

#[test]
fn f32x4_trunc_sat_signed_and_unsigned_match_wasm_saturation() {
    let mut signed = Vec::new();
    v128_const(
        &mut signed,
        f32x4([f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 42.9]),
    );
    simd(&mut signed, 248);
    assert_eq!(read_i32x4(run_v128(&signed)), [0, i32::MAX, i32::MIN, 42]);

    let mut unsigned = Vec::new();
    v128_const(&mut unsigned, f32x4([-1.0, f32::NAN, f32::INFINITY, 42.9]));
    simd(&mut unsigned, 249);
    assert_eq!(read_u32x4(run_v128(&unsigned)), [0, 0, u32::MAX, 42]);
}

#[test]
fn f32x4_integer_conversions_preserve_signed_and_unsigned_interpretation() {
    let mut signed = Vec::new();
    v128_const(&mut signed, i32x4([-16_777_216, -7, 0, 16_777_216]));
    simd(&mut signed, 250);
    assert_eq!(
        read_f32x4(run_v128(&signed)),
        [-16_777_216.0, -7.0, 0.0, 16_777_216.0]
    );

    let mut unsigned = Vec::new();
    v128_const(&mut unsigned, u32x4([0, 7, 16_777_216, u32::MAX]));
    simd(&mut unsigned, 251);
    let actual = read_f32x4(run_v128(&unsigned));
    assert_eq!(actual[0..3], [0.0, 7.0, 16_777_216.0]);
    assert_eq!(actual[3], u32::MAX as f32);
}

#[test]
fn f64x2_trunc_sat_zeroes_upper_i32_lanes() {
    let mut signed = Vec::new();
    v128_const(&mut signed, f64x2([f64::NEG_INFINITY, 19.75]));
    simd(&mut signed, 252);
    assert_eq!(read_i32x4(run_v128(&signed)), [i32::MIN, 19, 0, 0]);

    let mut unsigned = Vec::new();
    v128_const(&mut unsigned, f64x2([f64::NAN, f64::INFINITY]));
    simd(&mut unsigned, 253);
    assert_eq!(read_u32x4(run_v128(&unsigned)), [0, u32::MAX, 0, 0]);
}

#[test]
fn f64x2_convert_low_uses_only_low_i32x4_lanes() {
    let mut signed = Vec::new();
    v128_const(&mut signed, i32x4([-9, 17, 1234, -5678]));
    simd(&mut signed, 254);
    assert_eq!(read_f64x2(run_v128(&signed)), [-9.0, 17.0]);

    let mut unsigned = Vec::new();
    v128_const(&mut unsigned, u32x4([u32::MAX, 17, 1234, 5678]));
    simd(&mut unsigned, 255);
    assert_eq!(read_f64x2(run_v128(&unsigned)), [u32::MAX as f64, 17.0]);
}

#[test]
fn conversion_validates_and_scans_structured_control() {
    let mut instructions = vec![0x02, 0x7f];
    v128_const(&mut instructions, f32x4([5.9, 0.0, 0.0, 0.0]));
    simd(&mut instructions, 248);
    simd(&mut instructions, 27);
    instructions.push(0);
    instructions.push(0x0b);
    let parsed = parse_module(&module(0x7f, &instructions)).expect("structured fixture parses");
    let mut instance = Instance::new(parsed).expect("structured fixture validates");
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(5))
    );

    let mut bad = Vec::new();
    push_i32_const(&mut bad, 1);
    simd(&mut bad, 248);
    let parsed = parse_module(&module(0x7b, &bad)).expect("type-confusion fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}

#[test]
fn relaxed_simd_frontier_remains_fail_closed_at_257() {
    let mut instructions = Vec::new();
    v128_const(&mut instructions, [0; 16]);
    simd(&mut instructions, 259);
    let parsed = parse_module(&module(0x7b, &instructions)).expect("frontier fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 259,
                ..
            }
        ))
    ));
}
