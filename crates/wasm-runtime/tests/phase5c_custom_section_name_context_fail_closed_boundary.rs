use wasm_parser::{parse_module, ParseError};

const HEADER: [u8; 8] = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    assert!(payload.len() < 128);
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

fn empty_type_section(module: &mut Vec<u8>) {
    push_section(module, 1, &[0x01, 0x60, 0x00, 0x00]);
}

fn empty_function_section(module: &mut Vec<u8>) {
    push_section(module, 3, &[0x00]);
}

#[test]
fn malformed_custom_name_between_standard_sections_fails_closed() {
    let mut module = HEADER.to_vec();
    empty_type_section(&mut module);
    push_section(&mut module, 0, &[0x01, 0xff]);
    empty_function_section(&mut module);

    assert_eq!(parse_module(&module), Err(ParseError::InvalidUtf8));
}

#[test]
fn later_malformed_custom_name_is_not_hidden_by_an_earlier_valid_custom_section() {
    let mut module = HEADER.to_vec();
    push_section(&mut module, 0, &[0x01, b'a', 0xde, 0xad]);
    empty_type_section(&mut module);
    push_section(&mut module, 0, &[0x02, b'x']);

    assert_eq!(parse_module(&module), Err(ParseError::UnexpectedEof));
}

#[test]
fn malformed_custom_name_after_standard_sections_still_fails_closed() {
    let mut module = HEADER.to_vec();
    empty_type_section(&mut module);
    empty_function_section(&mut module);
    push_section(&mut module, 0, &[0x80]);

    assert_eq!(parse_module(&module), Err(ParseError::UnexpectedEof));
}
