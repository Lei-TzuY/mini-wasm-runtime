use wasm_parser::{parse_module, ParseError};

const HEADER: [u8; 8] = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

fn module_with_custom_payload(payload: &[u8]) -> Vec<u8> {
    assert!(payload.len() < 128);
    let mut module = HEADER.to_vec();
    module.push(0x00);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
    module
}

#[test]
fn custom_section_requires_a_name_field() {
    let module = module_with_custom_payload(&[]);
    assert_eq!(parse_module(&module), Err(ParseError::UnexpectedEof));
}

#[test]
fn malformed_custom_name_length_fails_closed() {
    for payload in [
        vec![0x80],
        vec![0x80, 0x80, 0x80, 0x80, 0x80],
        vec![0xff, 0xff, 0xff, 0xff, 0x10],
    ] {
        let module = module_with_custom_payload(&payload);
        let error = parse_module(&module).expect_err("malformed custom name length must fail");
        assert!(matches!(
            error,
            ParseError::UnexpectedEof | ParseError::InvalidLeb128 | ParseError::Leb128Overflow
        ));
    }
}

#[test]
fn truncated_custom_name_bytes_fail_closed() {
    let module = module_with_custom_payload(&[0x02, b'x']);
    assert_eq!(parse_module(&module), Err(ParseError::UnexpectedEof));
}

#[test]
fn invalid_utf8_custom_name_fails_closed() {
    let module = module_with_custom_payload(&[0x01, 0xff]);
    assert_eq!(parse_module(&module), Err(ParseError::InvalidUtf8));
}

#[test]
fn valid_noncanonical_custom_name_keeps_trailing_payload_opaque() {
    let mut module = module_with_custom_payload(&[
        0x81, 0x00, b'x', // non-minimal, width-valid name length = 1
        0x01, 0xff, 0x02, 0x00, // opaque custom payload that resembles section framing
    ]);
    module.extend_from_slice(&[0x01, 0x04, 0x01, 0x60, 0x00, 0x00]);

    let parsed =
        parse_module(&module).expect("valid custom section must not desynchronize parsing");
    assert_eq!(parsed.types.len(), 1);
    assert!(parsed.types[0].params.is_empty());
    assert!(parsed.types[0].results.is_empty());
}
