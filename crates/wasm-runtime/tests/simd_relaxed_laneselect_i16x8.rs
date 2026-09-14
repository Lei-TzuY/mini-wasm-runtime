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

fn v128_const(code: &mut Vec<u8>, value: [u8; 16]) {
    code.extend_from_slice(&[0xfd, 0x0c]);
    code.extend_from_slice(&value);
}

fn simd(code: &mut Vec<u8>, subopcode: u32) {
    code.push(0xfd);
    push_u32(code, subopcode);
}

fn execute_lane0(a: [u8; 16], b: [u8; 16], mask: [u8; 16]) -> i32 {
    let mut code = Vec::new();
    v128_const(&mut code, a);
    v128_const(&mut code, b);
    v128_const(&mut code, mask);
    simd(&mut code, 266);
    simd(&mut code, 25);
    code.push(0);
    let parsed = parse_module(&module(&code)).expect("lane-select fixture parses");
    let mut instance = Instance::new(parsed).expect("lane-select fixture validates");
    match instance
        .invoke_export_values("run", &[])
        .expect("lane-select executes")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected lane-select result: {other:?}"),
    }
}

#[test]
fn relaxed_laneselect_handles_deterministic_and_mixed_masks() {
    let a = [0xaa; 16];
    let b = [0x55; 16];
    assert_eq!(execute_lane0(a, b, [0xff; 16]), 0xaaaa);
    assert_eq!(execute_lane0(a, b, [0; 16]), 0x5555);
    let mut mixed = [0u8; 16];
    mixed[0] = 0xf0;
    mixed[1] = 0xf0;
    assert_eq!(execute_lane0(a, b, mixed), 0xa5a5);
}

#[test]
fn relaxed_laneselect_validates_three_v128_operands() {
    let mut code = Vec::new();
    v128_const(&mut code, [1; 16]);
    v128_const(&mut code, [2; 16]);
    code.extend_from_slice(&[0x41, 0x00]);
    simd(&mut code, 266);
    simd(&mut code, 25);
    code.push(0);
    let parsed = parse_module(&module(&code)).expect("type-confusion fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}

#[test]
fn next_relaxed_simd_subopcode_remains_fail_closed() {
    let mut code = Vec::new();
    v128_const(&mut code, [1; 16]);
    v128_const(&mut code, [2; 16]);
    v128_const(&mut code, [0xff; 16]);
    simd(&mut code, 276);
    simd(&mut code, 25);
    code.push(0);
    let parsed = parse_module(&module(&code)).expect("frontier fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 276,
                ..
            }
        ))
    ));
}
