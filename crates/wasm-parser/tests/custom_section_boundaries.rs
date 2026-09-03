use wasm_parser::{parse_module, ParseError};

fn module_with_custom(payload: &[u8]) -> Vec<u8> {
    assert!(payload.len() < 128);
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    module.push(0x00);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
    module
}

#[test]
fn accepts_well_formed_custom_section_names_and_opaque_payload() {
    // Empty names are valid names; bytes following the name are opaque custom data.
    parse_module(&module_with_custom(&[0x00])).expect("empty custom-section name is valid");
    parse_module(&module_with_custom(&[
        0x04, b'n', b'a', b'm', b'e', 0xff, 0x00, 0x80,
    ]))
    .expect("well-formed UTF-8 custom-section name with opaque payload is valid");
}

#[test]
fn rejects_custom_section_without_name_field() {
    assert_eq!(
        parse_module(&module_with_custom(&[])),
        Err(ParseError::UnexpectedEof)
    );
}

#[test]
fn rejects_truncated_custom_section_name() {
    assert_eq!(
        parse_module(&module_with_custom(&[0x03, b'a', b'b'])),
        Err(ParseError::UnexpectedEof)
    );
}

#[test]
fn rejects_invalid_utf8_custom_section_name() {
    assert_eq!(
        parse_module(&module_with_custom(&[0x02, 0xc3, 0x28])),
        Err(ParseError::InvalidUtf8)
    );
}

#[test]
fn rejects_overlong_custom_section_name_length() {
    assert_eq!(
        parse_module(&module_with_custom(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x00])),
        Err(ParseError::InvalidLeb128)
    );
}
