use wasm_parser::{parse_module, ValueType};
use wasm_runtime::{Instance, RuntimeError};
use wasm_validator::ValidationError;

const I32: u8 = 0x7f;
const I64: u8 = 0x7e;

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

    let mut type_section = vec![0x01, 0x60, 0x00];
    match result {
        Some(result) => type_section.extend([0x01, result]),
        None => type_section.push(0x00),
    }
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
    let module = parse_module(module).expect("negative fixture must remain structurally parseable");
    match Instance::new(module).expect_err(expectation) {
        RuntimeError::Validation(error) => error,
        other => panic!("expected validator rejection, got {other:?}"),
    }
}

#[test]
fn operand_stack_underflow_is_rejected() {
    let module = build_module(None, &[0x6a]); // i32.add with no operands
    assert!(matches!(
        validation_error(&module, "typed operators must not read below the operand stack"),
        ValidationError::OperandStackUnderflow { function: 0, .. }
    ));
}

#[test]
fn function_result_type_must_match_signature() {
    let module = build_module(
        Some(I64),
        &[
            0x41, 0x00, // i32.const 0, but the function returns i64
        ],
    );
    assert!(matches!(
        validation_error(&module, "function result must match its declared type"),
        ValidationError::TypeMismatch {
            function: 0,
            expected: ValueType::I64,
            actual: ValueType::I32,
            ..
        }
    ));
}

#[test]
fn block_result_requires_exact_stack_height() {
    let module = build_module(
        Some(I32),
        &[
            0x02, I32, // block (result i32)
            0x0b, // end without producing the result
        ],
    );
    assert!(matches!(
        validation_error(&module, "result block must produce exactly one value"),
        ValidationError::StackHeightMismatch {
            function: 0,
            expected: 1,
            actual: 0,
            ..
        }
    ));
}

#[test]
fn branch_value_must_match_target_label() {
    let module = build_module(
        Some(I64),
        &[
            0x02, I64, // block (result i64)
            0x41, 0x00, // i32.const 0: wrong label value type
            0x0c, 0x00, // br 0
            0x0b,
        ],
    );
    assert!(matches!(
        validation_error(&module, "branch operands must match the target label type"),
        ValidationError::TypeMismatch {
            function: 0,
            expected: ValueType::I64,
            actual: ValueType::I32,
            ..
        }
    ));
}

#[test]
fn if_condition_must_be_i32() {
    let module = build_module(
        None,
        &[
            0x42, 0x00, // i64.const 0: invalid condition type
            0x04, 0x40, // if with empty signature
            0x0b,
        ],
    );
    assert!(matches!(
        validation_error(&module, "if condition must be an i32"),
        ValidationError::TypeMismatch {
            function: 0,
            expected: ValueType::I32,
            actual: ValueType::I64,
            ..
        }
    ));
}

#[test]
fn result_if_requires_else() {
    let module = build_module(
        Some(I32),
        &[
            0x41, 0x01, // condition
            0x04, I32, // if (result i32)
            0x41, 0x07, // then result
            0x0b, // end if without else
        ],
    );
    assert!(matches!(
        validation_error(
            &module,
            "result-producing if without else must remain fail closed"
        ),
        ValidationError::MissingElseForResult { function: 0, .. }
    ));
}

#[test]
fn else_without_if_is_rejected() {
    let module = build_module(None, &[0x05]);
    assert!(matches!(
        validation_error(&module, "else must have a matching active if frame"),
        ValidationError::UnexpectedElse { function: 0, .. }
    ));
}

#[test]
fn duplicate_else_is_rejected() {
    let module = build_module(
        None,
        &[
            0x41, 0x01, // condition
            0x04, 0x40, // if with empty signature
            0x05, // first else
            0x05, // duplicate else
            0x0b,
        ],
    );
    assert!(matches!(
        validation_error(&module, "an if frame may contain at most one else"),
        ValidationError::DuplicateElse { function: 0, .. }
    ));
}

#[test]
fn function_end_cannot_be_followed_by_more_code() {
    let module = build_module(
        None,
        &[
            0x0b, // premature function end
            0x41, 0x00, // bytes after the final function end are invalid
        ],
    );
    assert!(matches!(
        validation_error(&module, "function end must terminate the body"),
        ValidationError::UnexpectedEnd { function: 0, .. }
    ));
}
