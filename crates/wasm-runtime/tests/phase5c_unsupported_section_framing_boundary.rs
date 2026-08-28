use wasm_parser::{parse_module, ParseError};

fn header() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

fn module_with_section(section_id: u8, length_and_payload: &[u8]) -> Vec<u8> {
    let mut module = header();
    module.push(section_id);
    module.extend_from_slice(length_and_payload);
    module
}

fn module_with_unsupported_section(length_and_payload: &[u8]) -> Vec<u8> {
    module_with_section(12, length_and_payload)
}

#[test]
fn malformed_unsupported_section_length_leb_fails_before_section_id_rejection() {
    assert_eq!(
        parse_module(&module_with_unsupported_section(&[0x80])),
        Err(ParseError::UnexpectedEof)
    );
    assert_eq!(
        parse_module(&module_with_unsupported_section(&[
            0x80, 0x80, 0x80, 0x80, 0x80,
        ])),
        Err(ParseError::InvalidLeb128)
    );
    assert_eq!(
        parse_module(&module_with_unsupported_section(&[
            0x80, 0x80, 0x80, 0x80, 0x10,
        ])),
        Err(ParseError::Leb128Overflow)
    );
}

#[test]
fn truncated_unsupported_section_payload_fails_before_section_id_rejection() {
    assert_eq!(
        parse_module(&module_with_unsupported_section(&[0x02, 0xaa])),
        Err(ParseError::UnexpectedEof)
    );
}

#[test]
fn well_framed_unsupported_section_rejects_the_section_id() {
    assert_eq!(
        parse_module(&module_with_unsupported_section(&[0x01, 0xaa])),
        Err(ParseError::UnsupportedSection(12))
    );
}

#[test]
fn every_future_standard_section_id_fails_closed() {
    for section_id in 12u8..=u8::MAX {
        assert_eq!(
            parse_module(&module_with_section(section_id, &[0x00])),
            Err(ParseError::UnsupportedSection(section_id)),
            "unsupported section id {section_id} must not be admitted"
        );
    }
}

#[test]
fn noncanonical_unsupported_section_length_still_reaches_section_id_rejection() {
    assert_eq!(
        parse_module(&module_with_unsupported_section(&[0x81, 0x00, 0xaa])),
        Err(ParseError::UnsupportedSection(12))
    );
}
