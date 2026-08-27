use wasm_parser::{parse_module, ParseError};

const TRUNCATED_U32: &[u8] = &[0x80];
const UNTERMINATED_U32: &[u8] = &[0x80, 0x80, 0x80, 0x80, 0x80];
const OVERFLOWING_U32: &[u8] = &[0x80, 0x80, 0x80, 0x80, 0x10];

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

fn assert_limit_error(id: u8, payload: &[u8], expected: ParseError) {
    assert_eq!(parse_module(&module_with_section(id, payload)), Err(expected));
}

fn table_min_payload(encoded: &[u8]) -> Vec<u8> {
    let mut payload = vec![0x01, 0x70, 0x00];
    payload.extend_from_slice(encoded);
    payload
}

fn table_max_payload(encoded: &[u8]) -> Vec<u8> {
    let mut payload = vec![0x01, 0x70, 0x01, 0x00];
    payload.extend_from_slice(encoded);
    payload
}

fn memory_min_payload(encoded: &[u8]) -> Vec<u8> {
    let mut payload = vec![0x01, 0x00];
    payload.extend_from_slice(encoded);
    payload
}

fn memory_max_payload(encoded: &[u8]) -> Vec<u8> {
    let mut payload = vec![0x01, 0x01, 0x00];
    payload.extend_from_slice(encoded);
    payload
}

fn assert_min_error(encoded: &[u8], expected: ParseError) {
    assert_limit_error(4, &table_min_payload(encoded), expected.clone());
    assert_limit_error(5, &memory_min_payload(encoded), expected);
}

fn assert_max_error(encoded: &[u8], expected: ParseError) {
    assert_limit_error(4, &table_max_payload(encoded), expected.clone());
    assert_limit_error(5, &memory_max_payload(encoded), expected);
}

#[test]
fn truncated_limit_minimums_fail_closed() {
    assert_min_error(TRUNCATED_U32, ParseError::UnexpectedEof);
}

#[test]
fn unterminated_limit_minimums_fail_closed() {
    assert_min_error(UNTERMINATED_U32, ParseError::InvalidLeb128);
}

#[test]
fn overflowing_limit_minimums_fail_closed() {
    assert_min_error(OVERFLOWING_U32, ParseError::Leb128Overflow);
}

#[test]
fn truncated_limit_maximums_fail_closed() {
    assert_max_error(TRUNCATED_U32, ParseError::UnexpectedEof);
}

#[test]
fn unterminated_limit_maximums_fail_closed() {
    assert_max_error(UNTERMINATED_U32, ParseError::InvalidLeb128);
}

#[test]
fn overflowing_limit_maximums_fail_closed() {
    assert_max_error(OVERFLOWING_U32, ParseError::Leb128Overflow);
}
