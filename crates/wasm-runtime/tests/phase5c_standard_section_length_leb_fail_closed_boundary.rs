use wasm_parser::{parse_module, ParseError};

const SECTION_IDS: [u8; 10] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 10];

fn module_with_raw_section_length(section_id: u8, length_bytes: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, section_id];
    module.extend_from_slice(length_bytes);
    module
}

fn assert_section_lengths_fail(length_bytes: &[u8], expected: ParseError) {
    for section_id in SECTION_IDS {
        assert_eq!(
            parse_module(&module_with_raw_section_length(section_id, length_bytes)),
            Err(expected.clone()),
            "section {section_id} must reject a malformed section-length LEB"
        );
    }
}

#[test]
fn truncated_standard_section_lengths_fail_closed() {
    assert_section_lengths_fail(&[0x80], ParseError::UnexpectedEof);
}

#[test]
fn unterminated_standard_section_lengths_fail_closed() {
    assert_section_lengths_fail(&[0x80, 0x80, 0x80, 0x80, 0x80], ParseError::InvalidLeb128);
}

#[test]
fn overflowing_standard_section_lengths_fail_closed() {
    assert_section_lengths_fail(&[0x80, 0x80, 0x80, 0x80, 0x10], ParseError::Leb128Overflow);
}

#[test]
fn noncanonical_zero_custom_section_length_remains_accepted() {
    let module = module_with_raw_section_length(0, &[0x80, 0x00]);
    parse_module(&module).expect("width-valid noncanonical section lengths must remain accepted");
}
