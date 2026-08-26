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

fn assert_section_count_error(id: u8, count_bytes: &[u8], expected: ParseError) {
    assert_eq!(
        parse_module(&module_with_section(id, count_bytes)),
        Err(expected)
    );
}

#[test]
fn truncated_segment_section_counts_fail_closed() {
    assert_section_count_error(9, &[0x80], ParseError::UnexpectedEof);
    assert_section_count_error(11, &[0x80], ParseError::UnexpectedEof);
}

#[test]
fn unterminated_segment_section_counts_fail_closed() {
    let malformed = [0x80, 0x80, 0x80, 0x80, 0x80];
    assert_section_count_error(9, &malformed, ParseError::InvalidLeb128);
    assert_section_count_error(11, &malformed, ParseError::InvalidLeb128);
}

#[test]
fn overflowing_segment_section_counts_fail_closed() {
    let overflow = [0x80, 0x80, 0x80, 0x80, 0x10];
    assert_section_count_error(9, &overflow, ParseError::Leb128Overflow);
    assert_section_count_error(11, &overflow, ParseError::Leb128Overflow);
}
