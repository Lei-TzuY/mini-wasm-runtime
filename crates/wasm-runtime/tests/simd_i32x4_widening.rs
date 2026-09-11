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
        let sign_bit_set = byte & 0x40 != 0;
        let done = (value == 0 && !sign_bit_set) || (value == -1 && sign_bit_set);
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

fn module(instructions: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x00, 0x01, 0x7f]);
    push_section(&mut bytes, 3, &[0x01, 0x00]);
    push_section(&mut bytes, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);

    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend(body);
    push_section(&mut bytes, 10, &code);
    bytes
}

fn push_simd(instructions: &mut Vec<u8>, subopcode: u32) {
    instructions.push(0xfd);
    push_u32(instructions, subopcode);
}

fn push_i16x8_const(instructions: &mut Vec<u8>, lanes: [i16; 8]) {
    push_simd(instructions, 12); // v128.const
    for lane in lanes {
        instructions.extend_from_slice(&lane.to_le_bytes());
    }
}

fn push_i32x4_extract(instructions: &mut Vec<u8>, lane: u8) {
    push_simd(instructions, 27); // i32x4.extract_lane
    instructions.push(lane);
}

fn run_i32(instructions: &[u8]) -> i32 {
    let parsed = parse_module(&module(instructions)).expect("i32x4 widening fixture must parse");
    let mut instance = Instance::new(parsed).expect("i32x4 widening fixture must validate");
    match instance
        .invoke_export_values("run", &[])
        .expect("i32x4 widening fixture must execute")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected i32x4 widening result: {other:?}"),
    }
}

#[test]
fn i32x4_extend_i16x8_low_high_signed_unsigned() {
    let lanes = [-32_768, 32_767, -1, 1, 2, 3, -2, -3];

    let mut low_s = Vec::new();
    push_i16x8_const(&mut low_s, lanes);
    push_simd(&mut low_s, 167); // i32x4.extend_low_i16x8_s
    push_i32x4_extract(&mut low_s, 0);
    assert_eq!(run_i32(&low_s), -32_768);

    let mut high_s = Vec::new();
    push_i16x8_const(&mut high_s, lanes);
    push_simd(&mut high_s, 168); // i32x4.extend_high_i16x8_s
    push_i32x4_extract(&mut high_s, 2);
    assert_eq!(run_i32(&high_s), -2);

    let mut low_u = Vec::new();
    push_i16x8_const(&mut low_u, lanes);
    push_simd(&mut low_u, 169); // i32x4.extend_low_i16x8_u
    push_i32x4_extract(&mut low_u, 2);
    assert_eq!(run_i32(&low_u), 65_535);

    let mut high_u = Vec::new();
    push_i16x8_const(&mut high_u, lanes);
    push_simd(&mut high_u, 170); // i32x4.extend_high_i16x8_u
    push_i32x4_extract(&mut high_u, 3);
    assert_eq!(run_i32(&high_u), 65_533);
}

#[test]
fn i32x4_widening_validates_and_scans_structured_control() {
    let mut instructions = vec![0x02, 0x7f]; // block (result i32)
    push_i16x8_const(&mut instructions, [-7, 1, 2, 3, 4, 5, 6, 7]);
    push_simd(&mut instructions, 167);
    push_i32x4_extract(&mut instructions, 0);
    instructions.push(0x0b);
    assert_eq!(run_i32(&instructions), -7);

    let mut bad = Vec::new();
    push_i32_const(&mut bad, 1);
    push_simd(&mut bad, 167);
    push_i32x4_extract(&mut bad, 0);
    let parsed = parse_module(&module(&bad)).expect("type-confusion fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}

#[test]
fn adjacent_f32x4_f64x2_min_frontier_remains_fail_closed() {
    let mut instructions = Vec::new();
    push_i16x8_const(&mut instructions, [1; 8]);
    push_simd(&mut instructions, 167);
    push_i32_const(&mut instructions, 1);
    push_simd(&mut instructions, 246); // f32x4 frontier remains outside this slice
    push_i32x4_extract(&mut instructions, 0);

    let parsed = parse_module(&module(&instructions)).expect("unsupported-SIMD fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 246,
                ..
            }
        ))
    ));
}
