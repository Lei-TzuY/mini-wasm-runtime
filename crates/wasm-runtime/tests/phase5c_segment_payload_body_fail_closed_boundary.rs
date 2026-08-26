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
    [0x01, 0x00, 0x41, 0x00, 0x0b]
}

fn element_payload(function_index_bytes: &[u8]) -> Vec<u8> {
    let mut payload = active_prefix().to_vec();
    payload.push(0x01); // one function index
    payload.extend_from_slice(function_index_bytes);
    payload
}

#[test]
fn data_payload_shorter_than_declared_length_fails_closed() {
    let mut payload = active_prefix().to_vec();
    payload.extend([0x02, 0xaa]); // declares two bytes, provides one
    assert_eq!(
        parse_module(&module_with_section(11, &payload)),
        Err(ParseError::UnexpectedEof)
    );
}

#[test]
fn truncated_element_function_index_fails_closed() {
    let payload = element_payload(&[0x80]);
    assert_eq!(
        parse_module(&module_with_section(9, &payload)),
        Err(ParseError::UnexpectedEof)
    );
}

#[test]
fn unterminated_element_function_index_fails_closed() {
    let payload = element_payload(&[0x80, 0x80, 0x80, 0x80, 0x80]);
    assert_eq!(
        parse_module(&module_with_section(9, &payload)),
        Err(ParseError::InvalidLeb128)
    );
}

#[test]
fn overflowing_element_function_index_fails_closed() {
    let payload = element_payload(&[0x80, 0x80, 0x80, 0x80, 0x10]);
    assert_eq!(
        parse_module(&module_with_section(9, &payload)),
        Err(ParseError::Leb128Overflow)
    );
}
