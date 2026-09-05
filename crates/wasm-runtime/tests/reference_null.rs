use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::ValidationError;

fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

fn module(body_ops: &[u8]) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 0, 1, 0x7f]);
    section(&mut module, 3, &[1, 0]);
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);
    let mut body = vec![0];
    body.extend_from_slice(body_ops);
    body.push(0x0b);
    let mut code = vec![1, body.len() as u8];
    code.extend_from_slice(&body);
    section(&mut module, 10, &code);
    module
}

#[test]
fn ref_null_funcref_is_null_executes() {
    let module = parse_module(&module(&[0xd0, 0x70, 0xd1])).unwrap();
    let mut instance = Instance::new(module).unwrap();
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(1))
    );
}

#[test]
fn ref_null_can_be_dropped() {
    let module = parse_module(&module(&[0xd0, 0x70, 0x1a, 0x41, 0x07])).unwrap();
    let mut instance = Instance::new(module).unwrap();
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(7))
    );
}

#[test]
fn ref_is_null_rejects_numeric_operand() {
    let module = parse_module(&module(&[0x41, 0x00, 0xd1])).unwrap();
    assert!(matches!(
        Instance::new(module),
        Err(RuntimeError::Validation(ValidationError::TypeMismatch {
            expected: wasm_parser::ValueType::FuncRef,
            actual: wasm_parser::ValueType::I32,
            ..
        }))
    ));
}

#[test]
fn ref_null_rejects_non_funcref_immediate() {
    let module = parse_module(&module(&[0xd0, 0x6f, 0xd1])).unwrap();
    assert!(matches!(
        Instance::new(module),
        Err(RuntimeError::Validation(
            ValidationError::MalformedImmediate { .. }
        ))
    ));
}
