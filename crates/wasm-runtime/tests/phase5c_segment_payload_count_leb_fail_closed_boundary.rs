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

fn active_prefix() -> [u8; 5] {
    [
        0x01, // one segment
        0x00, // active mode 0
        0x41, 0x00, // i32.const 0
        0x0b, // end offset expression
    ]
}

fn assert_data_error(count_bytes: &[u8], expected: ParseError) {
    let mut payload = active_prefix().to_vec();
    payload.extend_from_slice(count_bytes);
    assert_eq!(parse_module(&module_with_section(11, &payload)), Err(expected));
}

fn assert_element_error(count_bytes: &[u8], expected: ParseError) {
    let mut payload = active_prefix().to_vec();
    payload.extend_from_slice(count_bytes);
    assert_eq!(parse_module(&module_with_section(9, &payload)), Err(expected));
}

#[test]
fn truncated_payload_counts_fail_closed() {
    assert_data_error(&[0x80], ParseError::UnexpectedEof);
    assert_element_error(&[0x80], ParseError::UnexpectedEof);
}

#[test]
fn unterminated_payload_counts_fail_closed() {
    let malformed = [0x80, 0x80, 0x80, 0x80, 0x80];
    assert_data_error(&malformed, ParseError::InvalidLeb128);
    assert_element_error(&malformed, ParseError::InvalidLeb128);
}

#[test]
fn overflowing_payload_counts_fail_closed() {
    let overflow = [0x80, 0x80, 0x80, 0x80, 0x10];
    assert_data_error(&overflow, ParseError::Leb128Overflow);
    assert_element_error(&overflow, ParseError::Leb128Overflow);
}
