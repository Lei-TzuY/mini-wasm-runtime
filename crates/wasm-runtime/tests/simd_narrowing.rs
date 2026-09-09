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
fn module(instructions: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0, 0, 0];
    push_section(&mut bytes, 1, &[1, 0x60, 0, 1, 0x7f]);
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
fn simd(v: &mut Vec<u8>, op: u32) {
    v.push(0xfd);
    push_u32(v, op);
}
fn splat16(v: &mut Vec<u8>, x: i32) {
    push_i32_const(v, x);
    simd(v, 16);
}
fn splat32(v: &mut Vec<u8>, x: i32) {
    push_i32_const(v, x);
    simd(v, 17);
}
fn extract8s(v: &mut Vec<u8>, lane: u8) {
    simd(v, 21);
    v.push(lane);
}
fn extract8u(v: &mut Vec<u8>, lane: u8) {
    simd(v, 22);
    v.push(lane);
}
fn extract16s(v: &mut Vec<u8>, lane: u8) {
    simd(v, 24);
    v.push(lane);
}
fn extract16u(v: &mut Vec<u8>, lane: u8) {
    simd(v, 25);
    v.push(lane);
}
fn run(v: &[u8]) -> i32 {
    let parsed = parse_module(&module(v)).expect("narrowing fixture parses");
    let mut instance = Instance::new(parsed).expect("narrowing fixture validates");
    match instance
        .invoke_export_values("run", &[])
        .expect("narrowing executes")
        .as_slice()
    {
        [Value::I32(x)] => *x,
        other => panic!("unexpected narrowing result: {other:?}"),
    }
}

#[test]
fn i8x16_narrow_i16x8_s_and_u_saturate_and_preserve_operand_order() {
    let mut s = Vec::new();
    splat16(&mut s, -200);
    splat16(&mut s, 200);
    simd(&mut s, 101);
    extract8s(&mut s, 8);
    assert_eq!(run(&s), 127);
    let mut u = Vec::new();
    splat16(&mut u, -1);
    splat16(&mut u, 300);
    simd(&mut u, 102);
    extract8u(&mut u, 0);
    assert_eq!(run(&u), 0);
    let mut u_hi = Vec::new();
    splat16(&mut u_hi, 1);
    splat16(&mut u_hi, 300);
    simd(&mut u_hi, 102);
    extract8u(&mut u_hi, 15);
    assert_eq!(run(&u_hi), 255);
}

#[test]
fn i16x8_narrow_i32x4_s_and_u_saturate_and_preserve_operand_order() {
    let mut s = Vec::new();
    splat32(&mut s, -40000);
    splat32(&mut s, 40000);
    simd(&mut s, 133);
    extract16s(&mut s, 7);
    assert_eq!(run(&s), 32767);
    let mut u = Vec::new();
    splat32(&mut u, -1);
    splat32(&mut u, 70000);
    simd(&mut u, 134);
    extract16u(&mut u, 0);
    assert_eq!(run(&u), 0);
    let mut u_hi = Vec::new();
    splat32(&mut u_hi, 1);
    splat32(&mut u_hi, 70000);
    simd(&mut u_hi, 134);
    extract16u(&mut u_hi, 7);
    assert_eq!(run(&u_hi), 65535);
}

#[test]
fn narrowing_executes_inside_structured_control() {
    let mut v = vec![0x02, 0x7f];
    splat16(&mut v, 200);
    splat16(&mut v, -200);
    simd(&mut v, 101);
    extract8s(&mut v, 0);
    v.push(0x0b);
    assert_eq!(run(&v), 127);
}

#[test]
fn validator_rejects_narrowing_type_confusion() {
    let mut v = Vec::new();
    splat16(&mut v, 1);
    push_i32_const(&mut v, 2);
    simd(&mut v, 101);
    extract8u(&mut v, 0);
    let parsed = parse_module(&module(&v)).expect("type-confusion fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}
