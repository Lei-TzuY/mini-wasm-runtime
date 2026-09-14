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
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x00, 0x01, 0x7b]);
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
fn run(sub: u8, align: u8) -> [u8; 16] {
    let instructions = [0x41, 0x00, 0xfd, sub, align, 0x00];
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    let mut instance = Instance::new(parsed).expect("fixture validates");
    match instance
        .invoke_export_values("run", &[])
        .expect("executes")
        .remove(0)
    {
        Value::V128(bytes) => *bytes,
        other => panic!("unexpected {other:?}"),
    }
}
#[test]
fn zero_extending_loads_execute() {
    assert_eq!(
        run(0x5c, 2),
        [1, 2, 3, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    assert_eq!(
        run(0x5d, 3),
        [1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0, 0, 0, 0, 0]
    );
}
#[test]
fn validator_rejects_overaligned_zero_load() {
    let parsed =
        parse_module(&module(&[0x41, 0x00, 0xfd, 0x5c, 0x03, 0x00])).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::InvalidMemoryAlignment { .. }
        ))
    ));
}
#[test]
fn validator_rejects_zero_load_type_confusion() {
    let parsed =
        parse_module(&module(&[0x42, 0x00, 0xfd, 0x5c, 0x02, 0x00])).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}
