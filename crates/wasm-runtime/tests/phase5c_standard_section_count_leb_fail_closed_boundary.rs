use wasm_parser::{parse_module, ParseError};

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

fn module_with_section(id: u8, payload: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    module.push(id);
    push_u32(&mut module, payload.len() as u32);
    module.extend_from_slice(payload);
    module
}

fn assert_section_count_error(count_bytes: &[u8], expected: ParseError) {
    for (id, name) in [
        (1, "type"),
        (2, "import"),
        (3, "function"),
        (4, "table"),
        (5, "memory"),
        (6, "global"),
        (7, "export"),
    ] {
        assert_eq!(
            parse_module(&module_with_section(id, count_bytes)),
            Err(expected.clone()),
            "{name} section must reject malformed vector-count u32 LEB framing"
        );
    }
}

#[test]
fn truncated_standard_section_counts_fail_closed() {
    assert_section_count_error(&[0x80], ParseError::UnexpectedEof);
}

#[test]
fn unterminated_standard_section_counts_fail_closed() {
    assert_section_count_error(&[0x80, 0x80, 0x80, 0x80, 0x80], ParseError::InvalidLeb128);
}

#[test]
fn overflowing_standard_section_counts_fail_closed() {
    assert_section_count_error(&[0x80, 0x80, 0x80, 0x80, 0x10], ParseError::Leb128Overflow);
}
