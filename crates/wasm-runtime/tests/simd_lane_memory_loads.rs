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
fn module(result: u8, instructions: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x00, 0x01, result]);
    push_section(&mut bytes, 3, &[0x01, 0x00]);
    push_section(&mut bytes, 5, &[0x01, 0x00, 0x01]);
    push_section(&mut bytes, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);
    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend(body);
    push_section(&mut bytes, 10, &code);
    let data = [1u8, 2, 3, 4, 5, 6, 7, 8];
    let mut ds = vec![0x01, 0x00, 0x41, 0x00, 0x0b, data.len() as u8];
    ds.extend_from_slice(&data);
    push_section(&mut bytes, 11, &ds);
    bytes
}
fn zero_v128(i: &mut Vec<u8>) {
    i.extend_from_slice(&[0xfd, 0x0c]);
    i.extend_from_slice(&[0; 16]);
}
fn invoke(result: u8, sub: u8, align: u8, lane: u8, extract: u8) -> Value {
    let mut i = vec![0x41, 0x00];
    zero_v128(&mut i);
    i.extend_from_slice(&[0xfd, sub, align, 0x00, lane, 0xfd, extract, lane]);
    let parsed = parse_module(&module(result, &i)).expect("lane-load fixture parses");
    let mut instance = Instance::new(parsed).expect("lane-load fixture validates");
    instance
        .invoke_export_values("run", &[])
        .expect("lane-load executes")
        .remove(0)
}
#[test]
fn simd_lane_load_widths_execute() {
    assert_eq!(invoke(0x7f, 0x54, 0, 7, 0x16), Value::I32(1));
    assert_eq!(invoke(0x7f, 0x55, 1, 3, 0x19), Value::I32(0x0201));
    assert_eq!(invoke(0x7f, 0x56, 2, 2, 0x1b), Value::I32(0x04030201));
    assert_eq!(
        invoke(0x7e, 0x57, 3, 1, 0x1d),
        Value::I64(0x0807060504030201)
    );
}
#[test]
fn validator_rejects_lane_load_out_of_bounds() {
    let mut i = vec![0x41, 0x00];
    zero_v128(&mut i);
    i.extend_from_slice(&[0xfd, 0x54, 0x00, 0x00, 16]);
    let parsed = parse_module(&module(0x7b, &i)).expect("invalid lane fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::MalformedImmediate { .. }
        ))
    ));
}
#[test]
fn validator_rejects_lane_load_type_confusion() {
    let i = [0x41, 0x00, 0x41, 0x00, 0xfd, 0x54, 0x00, 0x00, 0x00];
    let parsed = parse_module(&module(0x7b, &i)).expect("type-confusion fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}
