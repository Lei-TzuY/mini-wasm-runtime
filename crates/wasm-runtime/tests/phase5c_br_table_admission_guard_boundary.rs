use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError};
use wasm_validator::ValidationError;

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    assert!(
        payload.len() < 128,
        "fixture helper only needs one-byte lengths"
    );
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

fn module_with_body(body: &[u8]) -> Vec<u8> {
    let type_section = [0x01, 0x60, 0x00, 0x00];

    let mut code_payload = vec![0x01, (body.len() + 1) as u8, 0x00];
    code_payload.extend_from_slice(body);

    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut bytes, 1, &type_section);
    push_section(&mut bytes, 3, &[0x01, 0x00]);
    push_section(&mut bytes, 10, &code_payload);
    bytes
}

fn assert_br_table_rejected(body: &[u8], expected_offset: usize) {
    let module = parse_module(&module_with_body(body))
        .expect("admission-guard fixture must remain structurally parseable");
    let error = Instance::new(module)
        .expect_err("br_table must remain fail-closed until its full vertical slice lands");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::UnsupportedOpcode {
            function: 0,
            offset,
            opcode: 0x0e,
        }) if offset == expected_offset
    ));
}

#[test]
fn non_i32_selector_cannot_partially_admit_br_table() {
    assert_br_table_rejected(
        &[
            0x42, 0x00, // i64.const 0: selector must eventually be i32
            0x0e, 0x00, 0x00, // br_table [] default 0
            0x0b,
        ],
        2,
    );
}

#[test]
fn mismatched_target_label_signatures_cannot_partially_admit_br_table() {
    assert_br_table_rejected(
        &[
            0x02, 0x7f, // outer block (result i32): depth 1 label requires i32
            0x02, 0x40, // inner block: depth 0 label requires no values
            0x41, 0x00, // i32.const selector
            0x0e, 0x01, 0x00, 0x01, // targets [0], default 1: label types disagree
            0x0b, // end inner block
            0x41, 0x00, // keep outer block structurally well-formed if validation continued
            0x0b, // end outer block
            0x1a, // drop outer result if validation continued
            0x0b,
        ],
        6,
    );
}

#[test]
fn out_of_bounds_target_depth_cannot_partially_admit_br_table() {
    assert_br_table_rejected(
        &[
            0x41, 0x00, // i32.const selector
            0x0e, 0x01, 0x01, 0x00, // target depth 1 is invalid at function scope
            0x0b,
        ],
        2,
    );
}

#[test]
fn unreachable_context_cannot_hide_unsupported_br_table() {
    assert_br_table_rejected(
        &[
            0x02, 0x40, // block
            0x0c, 0x00, // br 0 makes the rest of this block unreachable
            0x0e, 0x00, 0x00, // br_table [] default 0 must still be rejected
            0x0b, // end block
            0x0b,
        ],
        4,
    );
}
