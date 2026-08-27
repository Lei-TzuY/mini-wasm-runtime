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

fn module_with_start_payload(payload: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    module.push(8);
    push_u32(&mut module, payload.len() as u32);
    module.extend_from_slice(payload);
    module
}

fn assert_start_error(payload: &[u8], expected: ParseError) {
    assert_eq!(
        parse_module(&module_with_start_payload(payload)),
        Err(expected)
    );
}

#[test]
fn truncated_start_function_index_fails_closed() {
    assert_start_error(&[0x80], ParseError::UnexpectedEof);
}

#[test]
fn unterminated_start_function_index_fails_closed() {
    assert_start_error(&[0x80, 0x80, 0x80, 0x80, 0x80], ParseError::InvalidLeb128);
}

#[test]
fn overflowing_start_function_index_fails_closed() {
    assert_start_error(&[0x80, 0x80, 0x80, 0x80, 0x10], ParseError::Leb128Overflow);
}

#[test]
fn trailing_start_section_bytes_fail_closed() {
    assert_start_error(&[0x00, 0x00], ParseError::SectionLengthMismatch(8));
}

#[test]
fn noncanonical_start_function_index_leb_is_accepted() {
    let module = parse_module(&module_with_start_payload(&[0x80, 0x00]))
        .expect("noncanonical but width-valid u32 LEB must remain accepted");
    assert_eq!(module.start, Some(0));
}
