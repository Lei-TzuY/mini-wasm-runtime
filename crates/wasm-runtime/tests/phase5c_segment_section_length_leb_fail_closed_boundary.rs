use wasm_parser::{parse_module, ParseError};

fn module_with_raw_section_length(section_id: u8, length_bytes: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    module.push(section_id);
    module.extend_from_slice(length_bytes);
    module
}

fn assert_segment_section_lengths_fail(length_bytes: &[u8], expected: ParseError) {
    for section_id in [9, 11] {
        assert_eq!(
            parse_module(&module_with_raw_section_length(section_id, length_bytes)),
            Err(expected.clone()),
            "section {section_id} must reject a malformed section-length LEB"
        );
    }
}

#[test]
fn truncated_segment_section_lengths_fail_closed() {
    assert_segment_section_lengths_fail(&[0x80], ParseError::UnexpectedEof);
}

#[test]
fn unterminated_segment_section_lengths_fail_closed() {
    assert_segment_section_lengths_fail(&[0x80, 0x80, 0x80, 0x80, 0x80], ParseError::InvalidLeb128);
}

#[test]
fn overflowing_segment_section_lengths_fail_closed() {
    assert_segment_section_lengths_fail(
        &[0x80, 0x80, 0x80, 0x80, 0x10],
        ParseError::Leb128Overflow,
    );
}
