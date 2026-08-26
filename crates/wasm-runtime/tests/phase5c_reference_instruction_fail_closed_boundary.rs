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

fn module_with_instructions(instructions: &[u8]) -> Vec<u8> {
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

fn validator_error(bytes: &[u8], expectation: &str) -> ValidationError {
    let module = parse_module(bytes).expect("reference instruction boundary fixture must parse");
    match Instance::new(module).expect_err(expectation) {
        RuntimeError::Validation(error) => error,
        other => panic!("expected validator rejection, got {other:?}"),
    }
}

fn assert_unsupported(instructions: &[u8], opcode: u8, expectation: &str) {
    assert!(matches!(
        validator_error(&module_with_instructions(instructions), expectation),
        ValidationError::UnsupportedOpcode {
            function: 0,
            opcode: actual,
            ..
        } if actual == opcode
    ));
}

#[test]
fn ref_null_funcref_remains_fail_closed_at_instruction_boundary() {
    assert_unsupported(
        &[0xd0, 0x70],
        0xd0,
        "ref.null must remain outside the admitted instruction surface",
    );
}

#[test]
fn ref_is_null_remains_fail_closed_at_instruction_boundary() {
    assert_unsupported(
        &[0xd1],
        0xd1,
        "ref.is_null must remain outside the admitted instruction surface",
    );
}

#[test]
fn ref_func_remains_fail_closed_before_reference_values_are_admitted() {
    assert_unsupported(
        &[0xd2, 0x00],
        0xd2,
        "ref.func must remain outside the admitted instruction surface",
    );
}

#[test]
fn unreachable_polymorphism_does_not_hide_unsupported_reference_opcode() {
    assert_unsupported(
        &[
            0x02, 0x40, // block
            0x0c, 0x00, // br 0: remainder of this frame is unreachable
            0xd0, 0x70, // ref.null funcref must still fail closed
            0x0b,
        ],
        0xd0,
        "unreachable code must still reject unsupported reference instructions",
    );
}
