use wasm_parser::parse_module;
use wasm_validator::{validate, ValidationError};

const I32: u8 = 0x7f;

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

fn assert_typed_select_rejected(instructions: &[u8], expectation: &str) {
    let module = parse_module(&build_module(instructions)).expect("fixture must remain parseable");
    assert!(matches!(
        validate(&module),
        Err(ValidationError::UnsupportedOpcode {
            function: 0,
            opcode: 0x1c,
            ..
        })
    ), "{expectation}");
}

#[test]
fn typed_select_with_single_numeric_type_remains_fail_closed() {
    assert_typed_select_rejected(
        &[
            0x41, 0x01, // first i32 value
            0x41, 0x02, // second i32 value
            0x41, 0x00, // condition
            0x1c, 0x01, I32, // typed select [i32]
        ],
        "a well-formed typed select must not be partially admitted",
    );
}

#[test]
fn typed_select_empty_type_vector_is_rejected_at_the_opcode_boundary() {
    assert_typed_select_rejected(
        &[0x1c, 0x00],
        "unsupported typed select must fail closed before future immediate semantics leak in",
    );
}

#[test]
fn typed_select_truncated_immediate_is_rejected_at_the_opcode_boundary() {
    assert_typed_select_rejected(
        &[0x1c],
        "unsupported typed select must retain rejection precedence over its unimplemented immediate decoder",
    );
}

#[test]
fn unreachable_stack_polymorphism_does_not_admit_typed_select() {
    assert_typed_select_rejected(
        &[
            0x02, 0x40, // block
            0x0c, 0x00, // br 0 makes the rest of this frame unreachable
            0x1c, 0x01, I32, // typed select [i32]
            0x0b,
        ],
        "unreachable code must not hide an unsupported typed-select opcode",
    );
}
