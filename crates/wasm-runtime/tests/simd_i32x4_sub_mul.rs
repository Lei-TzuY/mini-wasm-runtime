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

fn run_lane0(instructions: &[u8]) -> i32 {
    let bytes = module(instructions);
    let parsed = parse_module(&bytes).expect("SIMD fixture must parse");
    let mut instance = Instance::new(parsed).expect("SIMD fixture must validate");
    match instance
        .invoke_export_values("run", &[])
        .expect("SIMD fixture must execute")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected result: {other:?}"),
    }
}

fn push_v128_const(instructions: &mut Vec<u8>, lanes: [i32; 4]) {
    instructions.extend_from_slice(&[0xfd, 0x0c]);
    for lane in lanes {
        instructions.extend_from_slice(&lane.to_le_bytes());
    }
}

#[test]
fn i32x4_sub_executes_with_wrapping_semantics() {
    let mut instructions = Vec::new();
    push_v128_const(&mut instructions, [i32::MIN, 7, -9, 12]);
    push_v128_const(&mut instructions, [1, 10, -4, 2]);
    instructions.extend_from_slice(&[0xfd, 0xb1, 0x01, 0xfd, 0x1b, 0x00]);
    assert_eq!(run_lane0(&instructions), i32::MAX);
}

#[test]
fn i32x4_mul_executes_with_wrapping_semantics() {
    let mut instructions = Vec::new();
    push_v128_const(&mut instructions, [0x4000_0000, -3, 7, 11]);
    push_v128_const(&mut instructions, [4, 9, -5, 13]);
    instructions.extend_from_slice(&[0xfd, 0xb5, 0x01, 0xfd, 0x1b, 0x00]);
    assert_eq!(run_lane0(&instructions), 0);
}

#[test]
fn validator_rejects_i32x4_sub_type_confusion() {
    let bytes = module(&[
        0x41, 0x01, // i32.const is not a v128 lhs
        0xfd, 0x0c, // v128.const rhs
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xfd, 0xb1, 0x01, // i32x4.sub
        0xfd, 0x1b, 0x00,
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
fn validator_rejects_i32x4_mul_type_confusion() {
    let bytes = module(&[
        0xfd, 0x0c, // v128.const lhs
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x41,
        0x01, // i32.const is not a v128 rhs
        0xfd, 0xb5, 0x01, // i32x4.mul
        0xfd, 0x1b, 0x00,
    ]);
    let parsed = parse_module(&bytes).expect("type-confusion fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}
