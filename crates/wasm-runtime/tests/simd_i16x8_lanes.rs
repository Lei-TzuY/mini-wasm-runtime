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

fn run_i32(instructions: &[u8]) -> i32 {
    let parsed = parse_module(&module(instructions)).expect("i16x8 lane fixture must parse");
    let mut instance = Instance::new(parsed).expect("i16x8 lane fixture must validate");
    match instance
        .invoke_export_values("run", &[])
        .expect("i16x8 lane fixture must execute")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected i16x8 lane result: {other:?}"),
    }
}

#[test]
fn i16x8_splat_extracts_signed_and_unsigned_lanes() {
    let mut signed = Vec::new();
    push_i32_const(&mut signed, -1);
    push_simd(&mut signed, 16); // i16x8.splat
    push_simd(&mut signed, 24); // i16x8.extract_lane_s
    signed.push(3);
    assert_eq!(run_i32(&signed), -1);

    let mut unsigned = Vec::new();
    push_i32_const(&mut unsigned, -1);
    push_simd(&mut unsigned, 16); // i16x8.splat
    push_simd(&mut unsigned, 25); // i16x8.extract_lane_u
    unsigned.push(6);
    assert_eq!(run_i32(&unsigned), 65_535);
}

#[test]
fn i16x8_splat_and_replace_lane_keep_only_low_16_bits() {
    let mut splat = Vec::new();
    push_i32_const(&mut splat, 0x1_2345);
    push_simd(&mut splat, 16); // i16x8.splat
    push_simd(&mut splat, 25); // i16x8.extract_lane_u
    splat.push(0);
    assert_eq!(run_i32(&splat), 0x2345);

    let mut replaced = Vec::new();
    push_i32_const(&mut replaced, 7);
    push_simd(&mut replaced, 16); // i16x8.splat
    push_i32_const(&mut replaced, 0x1_ffff);
    push_simd(&mut replaced, 26); // i16x8.replace_lane
    replaced.push(7);
    push_simd(&mut replaced, 25); // i16x8.extract_lane_u
    replaced.push(7);
    assert_eq!(run_i32(&replaced), 65_535);

    let mut untouched = Vec::new();
    push_i32_const(&mut untouched, 7);
    push_simd(&mut untouched, 16); // i16x8.splat
    push_i32_const(&mut untouched, 0x1_ffff);
    push_simd(&mut untouched, 26); // i16x8.replace_lane
    untouched.push(7);
    push_simd(&mut untouched, 25); // i16x8.extract_lane_u
    untouched.push(0);
    assert_eq!(run_i32(&untouched), 7);
}

#[test]
fn i16x8_lane_ops_execute_inside_structured_control() {
    let mut instructions = vec![0x02, 0x7f]; // block (result i32)
    push_i32_const(&mut instructions, -32_768);
    push_simd(&mut instructions, 16); // i16x8.splat
    push_simd(&mut instructions, 24); // i16x8.extract_lane_s
    instructions.push(5);
    instructions.push(0x0b); // end block
    assert_eq!(run_i32(&instructions), -32_768);
}

#[test]
fn validator_rejects_out_of_bounds_i16x8_lane() {
    let mut instructions = Vec::new();
    push_i32_const(&mut instructions, 0);
    push_simd(&mut instructions, 16); // i16x8.splat
    push_simd(&mut instructions, 25); // i16x8.extract_lane_u
    instructions.push(8);

    let parsed = parse_module(&module(&instructions)).expect("invalid-lane fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::MalformedImmediate { .. }
        ))
    ));
}

#[test]
fn validator_rejects_i16x8_splat_type_confusion() {
    let bytes = module(&[
        0x42, 0x00, // i64.const 0
        0xfd, 0x10, // i16x8.splat requires i32
        0xfd, 0x19, 0x00, // i16x8.extract_lane_u 0 if typing were wrong
    ]);
    let parsed = parse_module(&bytes).expect("type-confusion fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}

#[test]
fn validator_rejects_i16x8_replace_lane_type_confusion() {
    let bytes = module(&[
        0x41, 0x00, // i32.const 0
        0xfd, 0x10, // i16x8.splat
        0x42, 0x00, // i64.const 0 where replace_lane requires i32
        0xfd, 0x1a, 0x03, // i16x8.replace_lane 3
        0xfd, 0x19, 0x03, // extract result if typing were wrong
    ]);
    let parsed = parse_module(&bytes).expect("replace-lane type-confusion fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}

#[test]
fn adjacent_i16x8_comparison_remains_fail_closed() {
    let bytes = module(&[
        0xfd, 0x0c, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // v128.const
        0xfd, 0x0c, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // v128.const
        0xfd, 0x2d, // i16x8.eq remains outside this bounded slice
        0x1a, // drop result if an implementation accidentally skips the opcode
        0x41, 0x00, // i32.const 0
    ]);
    let parsed = parse_module(&bytes).expect("unsupported-SIMD fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 45,
                ..
            }
        ))
    ));
}
