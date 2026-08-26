use wasm_parser::{parse_module, ParseError};

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

fn module_with_segment_mode(section_id: u8, mode_bytes: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let mut payload = vec![0x01]; // one segment
    payload.extend_from_slice(mode_bytes);
    push_section(&mut module, section_id, &payload);
    module
}

fn assert_both_segment_sections_fail(mode_bytes: &[u8], expected: ParseError) {
    for (kind, section_id) in [("element", 9), ("data", 11)] {
        let module = module_with_segment_mode(section_id, mode_bytes);
        assert_eq!(
            parse_module(&module),
            Err(expected.clone()),
            "unexpected parser result for malformed {kind} segment mode"
        );
    }
}

#[test]
fn truncated_segment_mode_leb_fails_at_discriminant_decode() {
    assert_both_segment_sections_fail(&[0x80], ParseError::UnexpectedEof);
}

#[test]
fn unterminated_five_byte_segment_mode_leb_is_rejected() {
    assert_both_segment_sections_fail(&[0x80, 0x80, 0x80, 0x80, 0x80], ParseError::InvalidLeb128);
}

#[test]
fn overflowing_segment_mode_leb_is_rejected() {
    assert_both_segment_sections_fail(&[0x80, 0x80, 0x80, 0x80, 0x10], ParseError::Leb128Overflow);
}
