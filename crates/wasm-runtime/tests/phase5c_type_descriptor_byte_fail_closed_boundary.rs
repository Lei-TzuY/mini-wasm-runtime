use wasm_parser::{parse_module, ParseError};

fn module_with_section(id: u8, payload: &[u8]) -> Vec<u8> {
    assert!(payload.len() < 128);
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
    module
}

#[test]
fn missing_function_type_tag_fails_closed() {
    assert_eq!(
        parse_module(&module_with_section(1, &[0x01])),
        Err(ParseError::UnexpectedEof)
    );
}

#[test]
fn invalid_function_type_tag_is_rejected_before_signature_parsing() {
    assert_eq!(
        parse_module(&module_with_section(1, &[0x01, 0x61])),
        Err(ParseError::InvalidFunctionType(0x61))
    );
}

#[test]
fn missing_function_parameter_type_fails_closed() {
    assert_eq!(
        parse_module(&module_with_section(1, &[0x01, 0x60, 0x01])),
        Err(ParseError::UnexpectedEof)
    );
}

#[test]
fn missing_function_result_type_fails_closed() {
    assert_eq!(
        parse_module(&module_with_section(1, &[0x01, 0x60, 0x00, 0x01])),
        Err(ParseError::UnexpectedEof)
    );
}

#[test]
fn unsupported_numeric_signature_types_fail_closed() {
    assert_eq!(
        parse_module(&module_with_section(1, &[0x01, 0x60, 0x01, 0x7b, 0x00])),
        Err(ParseError::UnsupportedValueType(0x7b))
    );
    assert_eq!(
        parse_module(&module_with_section(1, &[0x01, 0x60, 0x00, 0x01, 0x7b])),
        Err(ParseError::UnsupportedValueType(0x7b))
    );
}

#[test]
fn missing_defined_global_value_type_fails_closed() {
    assert_eq!(
        parse_module(&module_with_section(6, &[0x01])),
        Err(ParseError::UnexpectedEof)
    );
}

#[test]
fn missing_imported_global_value_type_fails_closed() {
    let payload = [
        0x01, // one import
        0x00, // empty module name
        0x00, // empty field name
        0x03, // global import
    ];
    assert_eq!(
        parse_module(&module_with_section(2, &payload)),
        Err(ParseError::UnexpectedEof)
    );
}

#[test]
fn missing_table_reference_type_fails_closed() {
    assert_eq!(
        parse_module(&module_with_section(4, &[0x01])),
        Err(ParseError::UnexpectedEof)
    );

    let import = [
        0x01, // one import
        0x00, // empty module name
        0x00, // empty field name
        0x01, // table import
    ];
    assert_eq!(
        parse_module(&module_with_section(2, &import)),
        Err(ParseError::UnexpectedEof)
    );
}
