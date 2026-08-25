use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError};
use wasm_validator::ValidationError;

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    assert!(payload.len() < 128, "fixture helper only needs one-byte lengths");
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

fn module_with_body(results: u8, body: &[u8]) -> Vec<u8> {
    let mut type_section = vec![0x01, 0x60, 0x00, results];
    type_section.extend(std::iter::repeat(0x7f).take(results as usize));

    let mut code_payload = vec![0x01, (body.len() + 1) as u8, 0x00];
    code_payload.extend_from_slice(body);

    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut bytes, 1, &type_section);
    push_section(&mut bytes, 3, &[0x01, 0x00]);
    push_section(&mut bytes, 10, &code_payload);
    bytes
}

fn assert_br_table_rejected(body: &[u8], expected_offset: usize) {
    let bytes = module_with_body(0, body);
    let module = parse_module(&bytes).expect("boundary fixture must remain structurally parseable");
    let error = Instance::new(module).expect_err("br_table must remain fail-closed until fully admitted");
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
fn valid_looking_zero_target_br_table_remains_fail_closed() {
    assert_br_table_rejected(
        &[
            0x41, 0x00, // i32.const selector
            0x0e, 0x00, 0x00, // br_table [] default 0
            0x0b,
        ],
        2,
    );
}

#[test]
fn valid_looking_multi_target_br_table_remains_fail_closed() {
    assert_br_table_rejected(
        &[
            0x02, 0x40, // block
            0x02, 0x40, // block
            0x41, 0x01, // i32.const selector
            0x0e, 0x02, 0x00, 0x01, 0x01, // targets [0, 1], default 1
            0x0b, // end inner block
            0x0b, // end outer block
            0x0b,
        ],
        6,
    );
}

#[test]
fn malformed_target_vector_cannot_enter_partial_br_table_support() {
    assert_br_table_rejected(
        &[
            0x41, 0x00, // i32.const selector
            0x0e, 0x80, 0x80, 0x80, 0x80, 0x80, // malformed target-count LEB128
            0x0b,
        ],
        2,
    );
}

#[test]
fn br_table_rejection_precedes_following_supported_instructions() {
    assert_br_table_rejected(
        &[
            0x41, 0x00, // i32.const selector
            0x0e, 0x00, 0x00, // br_table [] default 0
            0x41, 0x2a, // otherwise-valid i32.const 42
            0x1a, // unsupported drop must never become the primary boundary
            0x0b,
        ],
        2,
    );
}
