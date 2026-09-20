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

fn push_i32(bytes: &mut Vec<u8>, mut value: i32) {
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

fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn module(instructions: &[u8]) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 0, 1, 0x7f]);
    section(&mut module, 3, &[1, 0]);
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);

    let mut body = vec![0];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![1];
    push_u32(&mut code, body.len() as u32);
    code.extend(body);
    section(&mut module, 10, &code);
    module
}

fn simd(code: &mut Vec<u8>, subopcode: u32) {
    code.push(0xfd);
    push_u32(code, subopcode);
}

fn push_i8x16(code: &mut Vec<u8>, lanes: [i8; 16]) {
    simd(code, 12);
    code.extend(lanes.map(|lane| lane as u8));
}

fn run_i32(code: &[u8]) -> i32 {
    let parsed = parse_module(&module(code)).expect("i8x16 wrapping fixture parses");
    let mut instance = Instance::new(parsed).expect("i8x16 wrapping fixture validates");
    match instance
        .invoke_export_values("run", &[])
        .expect("i8x16 wrapping fixture executes")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected i8x16 wrapping result: {other:?}"),
    }
}

fn lane(lhs: [i8; 16], rhs: [i8; 16], subopcode: u32, lane: u8, signed: bool) -> i32 {
    let mut code = Vec::new();
    push_i8x16(&mut code, lhs);
    push_i8x16(&mut code, rhs);
    simd(&mut code, subopcode);
    simd(&mut code, if signed { 21 } else { 22 });
    code.push(lane);
    run_i32(&code)
}

#[test]
fn i8x16_add_wraps_each_byte_lane() {
    let lhs = [127, -128, -1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    let rhs = [
        1, -1, 1, -1, 127, 126, 125, 124, 123, 122, 121, 120, 119, 118, 117, 116,
    ];

    assert_eq!(lane(lhs, rhs, 110, 0, true), -128);
    assert_eq!(lane(lhs, rhs, 110, 1, true), 127);
    assert_eq!(lane(lhs, rhs, 110, 2, false), 0);
    assert_eq!(lane(lhs, rhs, 110, 3, false), 255);
}

#[test]
fn i8x16_sub_wraps_each_byte_lane() {
    let lhs = [
        -128, 0, 127, -1, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120,
    ];
    let rhs = [
        1, 1, -1, 1, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 127,
    ];

    assert_eq!(lane(lhs, rhs, 113, 0, true), 127);
    assert_eq!(lane(lhs, rhs, 113, 1, false), 255);
    assert_eq!(lane(lhs, rhs, 113, 2, true), -128);
    assert_eq!(lane(lhs, rhs, 113, 3, false), 254);
}

#[test]
fn i8x16_wrapping_arithmetic_executes_inside_structured_control() {
    let mut code = vec![0x02, 0x7f];
    push_i8x16(&mut code, [127; 16]);
    push_i8x16(&mut code, [1; 16]);
    simd(&mut code, 110);
    simd(&mut code, 21);
    code.push(7);
    code.push(0x0b);
    assert_eq!(run_i32(&code), -128);
}

#[test]
fn validator_rejects_i8x16_wrapping_type_confusion() {
    let mut code = Vec::new();
    push_i8x16(&mut code, [1; 16]);
    push_i32(&mut code, 2);
    simd(&mut code, 110);
    simd(&mut code, 22);
    code.push(0);

    let parsed = parse_module(&module(&code)).expect("type-confusion fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}
