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

fn build_module(result: Option<u8>, instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let type_section = match result {
        Some(value_type) => vec![0x01, 0x60, 0x00, 0x01, value_type],
        None => vec![0x01, 0x60, 0x00, 0x00],
    };
    push_section(&mut module, 1, &type_section);
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
fn reachable_br_if_requires_i32_condition() {
    let module = build_module(
        None,
        &[
            0x42, 0x00, // i64.const 0
            0x0d, 0x00, // br_if function label
        ],
    );
    assert!(matches!(
        validation_error(&module, "br_if condition must be i32"),
        ValidationError::TypeMismatch {
            function: 0,
            expected: wasm_parser::ValueType::I32,
            actual: wasm_parser::ValueType::I64,
            ..
        }
    ));
}

#[test]
fn reachable_branch_must_supply_target_result() {
    let module = build_module(
        None,
        &[
            0x02, 0x7f, // block (result i32)
            0x0c, 0x00, // br 0 without result value
            0x0b, 0x1a, // end; drop if validation continued
        ],
    );
    assert!(matches!(
        validation_error(&module, "branch must supply the target label value"),
        ValidationError::OperandStackUnderflow { function: 0, .. }
    ));
}

#[test]
fn reachable_branch_result_type_must_match_target() {
    let module = build_module(
        None,
        &[
            0x02, 0x7f, // block (result i32)
            0x42, 0x00, // i64.const 0
            0x0c, 0x00, // br 0
            0x0b, 0x1a, // end; drop if validation continued
        ],
    );
    assert!(matches!(
        validation_error(&module, "branch result must match the target label type"),
        ValidationError::TypeMismatch {
            function: 0,
            expected: wasm_parser::ValueType::I32,
            actual: wasm_parser::ValueType::I64,
            ..
        }
    ));
}

#[test]
fn unreachable_code_still_checks_branch_depth() {
    let module = build_module(
        None,
        &[
            0x02, 0x40, // block
            0x0c, 0x00, // br 0 makes the rest of the block unreachable
            0x0c, 0x02, // depth 2 is still invalid: only block + function exist
            0x0b,
        ],
    );
    assert!(matches!(
        validation_error(
            &module,
            "unreachable code must still validate branch depths"
        ),
        ValidationError::BranchDepthOutOfBounds {
            function: 0,
            depth: 2,
            ..
        }
    ));
}

#[test]
fn return_must_supply_function_result_type() {
    let module = build_module(
        Some(0x7f),
        &[
            0x42, 0x00, // i64.const 0
            0x0f, // return from an i32-result function
        ],
    );
    assert!(matches!(
        validation_error(&module, "return value must match the function result type"),
        ValidationError::TypeMismatch {
            function: 0,
            expected: wasm_parser::ValueType::I32,
            actual: wasm_parser::ValueType::I64,
            ..
        }
    ));
}
