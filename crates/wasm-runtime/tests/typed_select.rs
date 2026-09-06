use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::ValidationError;

fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    assert!(payload.len() < 128);
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

fn instantiate(body_ops: &[u8]) -> Result<Instance, RuntimeError> {
    Instance::new(parse_module(&module(body_ops)).expect("parse typed-select test module"))
}

#[test]
fn typed_select_i32_chooses_both_branches() {
    for (condition, expected) in [(1, 10), (0, 20)] {
        let mut vm =
            instantiate(&[0x41, 0x0a, 0x41, 0x14, 0x41, condition, 0x1c, 0x01, 0x7f]).unwrap();
        assert_eq!(
            vm.invoke_export("run", &[]).unwrap(),
            Some(Value::I32(expected))
        );
    }
}

#[test]
fn typed_select_funcref_preserves_nullability() {
    let mut non_null =
        instantiate(&[0xd2, 0x00, 0xd0, 0x70, 0x41, 0x01, 0x1c, 0x01, 0x70, 0xd1]).unwrap();
    assert_eq!(
        non_null.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(0))
    );

    let mut null =
        instantiate(&[0xd2, 0x00, 0xd0, 0x70, 0x41, 0x00, 0x1c, 0x01, 0x70, 0xd1]).unwrap();
    assert_eq!(null.invoke_export("run", &[]).unwrap(), Some(Value::I32(1)));
}

#[test]
fn typed_select_rejects_operand_not_matching_declared_type() {
    let error = instantiate(&[0x41, 0x01, 0x42, 0x02, 0x41, 0x01, 0x1c, 0x01, 0x7f])
        .expect_err("declared i32 select must reject an i64 operand");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::TypeMismatch { .. })
    ));
}

#[test]
fn typed_select_rejects_non_singleton_type_vector() {
    for immediate in [&[0x00][..], &[0x02, 0x7f, 0x7f][..]] {
        let mut body = vec![0x41, 0x01, 0x41, 0x02, 0x41, 0x01, 0x1c];
        body.extend_from_slice(immediate);
        let error = instantiate(&body).expect_err("typed select requires one result type");
        assert!(matches!(
            error,
            RuntimeError::Validation(ValidationError::MalformedImmediate { .. })
        ));
    }
}

#[test]
fn typed_select_rejects_unsupported_result_type() {
    let error = instantiate(&[0x41, 0x01, 0x41, 0x02, 0x41, 0x01, 0x1c, 0x01, 0x6f])
        .expect_err("unsupported reference type must fail closed");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::MalformedImmediate { .. })
    ));
}
