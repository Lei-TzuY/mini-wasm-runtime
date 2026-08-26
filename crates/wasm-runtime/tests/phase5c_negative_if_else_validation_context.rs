use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError};
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

fn build_module(instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x00]);
    push_section(&mut module, 3, &[0x01, 0x00]);

    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn validation_error(module: &[u8], expectation: &str) -> ValidationError {
    let module = parse_module(module).expect("fixture must remain structurally parseable");
    match Instance::new(module).expect_err(expectation) {
        RuntimeError::Validation(error) => error,
        other => panic!("expected validator rejection, got {other:?}"),
    }
}

#[test]
fn result_if_without_else_is_rejected_even_when_then_produces_result() {
    let module = build_module(&[
        0x41, 0x01, // i32.const 1: condition
        0x04, 0x7f, // if (result i32)
        0x41, 0x2a, // i32.const 42
        0x0b,
        0x1a, // drop the result if validation were to continue
    ]);
    assert!(matches!(
        validation_error(&module, "result-typed if must have an else arm"),
        ValidationError::MissingElseForResult {
            function: 0,
            ..
        }
    ));
}

#[test]
fn duplicate_else_is_rejected_before_the_second_arm_is_admitted() {
    let module = build_module(&[
        0x41, 0x01, // i32.const 1: condition
        0x04, 0x40, // if
        0x05, // else
        0x05, // duplicate else
        0x0b,
    ]);
    assert!(matches!(
        validation_error(&module, "an if may contain at most one else marker"),
        ValidationError::DuplicateElse {
            function: 0,
            ..
        }
    ));
}

#[test]
fn else_without_matching_if_is_rejected() {
    let module = build_module(&[0x05]);
    assert!(matches!(
        validation_error(&module, "else outside an if must fail closed"),
        ValidationError::UnexpectedElse {
            function: 0,
            ..
        }
    ));
}

#[test]
fn reachable_else_arm_must_produce_the_declared_result_type() {
    let module = build_module(&[
        0x41, 0x01, // i32.const 1: condition
        0x04, 0x7f, // if (result i32)
        0x41, 0x2a, // then: i32.const 42
        0x05, // else
        0x42, 0x2a, // else: i64.const 42
        0x0b,
        0x1a,
    ]);
    assert!(matches!(
        validation_error(&module, "else result type must match the if signature"),
        ValidationError::TypeMismatch {
            function: 0,
            ..
        }
    ));
}
