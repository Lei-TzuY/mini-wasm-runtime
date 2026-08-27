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

fn module_with_type_payload(payload: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01];
    push_u32(&mut module, payload.len() as u32);
    module.extend_from_slice(payload);
    module
}

fn assert_param_count_error(count_bytes: &[u8], expected: ParseError) {
    let mut payload = vec![0x01, 0x60];
    payload.extend_from_slice(count_bytes);
    assert_eq!(
        parse_module(&module_with_type_payload(&payload)),
        Err(expected)
    );
}

fn assert_result_count_error(count_bytes: &[u8], expected: ParseError) {
    let mut payload = vec![0x01, 0x60, 0x00];
    payload.extend_from_slice(count_bytes);
    assert_eq!(
        parse_module(&module_with_type_payload(&payload)),
        Err(expected)
    );
}

#[test]
fn truncated_signature_counts_fail_closed() {
    assert_param_count_error(&[0x80], ParseError::UnexpectedEof);
    assert_result_count_error(&[0x80], ParseError::UnexpectedEof);
}

#[test]
fn unterminated_signature_counts_fail_closed() {
    let malformed = [0x80, 0x80, 0x80, 0x80, 0x80];
    assert_param_count_error(&malformed, ParseError::InvalidLeb128);
    assert_result_count_error(&malformed, ParseError::InvalidLeb128);
}

#[test]
fn overflowing_signature_counts_fail_closed() {
    let overflow = [0x80, 0x80, 0x80, 0x80, 0x10];
    assert_param_count_error(&overflow, ParseError::Leb128Overflow);
    assert_result_count_error(&overflow, ParseError::Leb128Overflow);
}

#[test]
fn noncanonical_zero_signature_counts_remain_accepted() {
    let payload = [0x01, 0x60, 0x80, 0x00, 0x80, 0x00];
    let module = parse_module(&module_with_type_payload(&payload))
        .expect("width-valid noncanonical u32 LEB counts must remain accepted");
    assert_eq!(module.types.len(), 1);
    assert!(module.types[0].params.is_empty());
    assert!(module.types[0].results.is_empty());
}
