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

fn module_with_body(results: &[u8], body: &[u8]) -> Vec<u8> {
    let mut type_section = vec![0x01, 0x60, 0x00, results.len() as u8];
    type_section.extend_from_slice(results);

    let mut code_payload = vec![0x01, (body.len() + 1) as u8, 0x00];
    code_payload.extend_from_slice(body);

    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut bytes, 1, &type_section);
    push_section(&mut bytes, 3, &[0x01, 0x00]);
    push_section(&mut bytes, 10, &code_payload);
    bytes
}

fn assert_br_table_rejected(results: &[u8], body: &[u8], expected_offset: usize) {
    let module = parse_module(&module_with_body(results, body))
        .expect("boundary fixture must remain structurally parseable");
    let error =
        Instance::new(module).expect_err("br_table must remain fail-closed until fully admitted");
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
fn negative_selector_remains_fail_closed_before_unsigned_dispatch_is_admitted() {
    assert_br_table_rejected(
        &[],
        &[
            0x02, 0x40, // block
            0x41, 0x7f, // i32.const -1
            0x0e, 0x01, 0x00, 0x00, // br_table [0] default 0
            0x0b, // end block
            0x0b,
        ],
        4,
    );
}

#[test]
fn result_value_below_selector_remains_fail_closed_until_label_typing_is_admitted() {
    assert_br_table_rejected(
        &[0x7f],
        &[
            0x02, 0x7f, // block (result i32)
            0x41, 0x2a, // branch value 42
            0x41, 0x00, // selector 0
            0x0e, 0x00, 0x00, // br_table [] default 0
            0x0b, // end block
            0x0b,
        ],
        6,
    );
}
