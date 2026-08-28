use wasm_parser::{parse_module, ParseError, ValueType};

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

fn module_with_code_payload(payload: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    module.push(10);
    push_u32(&mut module, payload.len() as u32);
    module.extend_from_slice(payload);
    module
}

fn module_with_body(body: &[u8]) -> Vec<u8> {
    let mut payload = vec![0x01];
    push_u32(&mut payload, body.len() as u32);
    payload.extend_from_slice(body);
    module_with_code_payload(&payload)
}

fn assert_code_framing_error(leb: &[u8], expected: ParseError) {
    let section_count = module_with_code_payload(leb);

    let mut body_length_payload = vec![0x01];
    body_length_payload.extend_from_slice(leb);
    let body_length = module_with_code_payload(&body_length_payload);

    let local_group_count = module_with_body(leb);

    let mut local_count_body = vec![0x01];
    local_count_body.extend_from_slice(leb);
    let local_count = module_with_body(&local_count_body);

    for (name, module) in [
        ("code vector count", section_count),
        ("function body length", body_length),
        ("local group count", local_group_count),
        ("local declaration count", local_count),
    ] {
        assert_eq!(
            parse_module(&module),
            Err(expected.clone()),
            "{name} must reject malformed u32 LEB framing"
        );
    }
}

#[test]
fn truncated_code_section_immediates_fail_closed() {
    assert_code_framing_error(&[0x80], ParseError::UnexpectedEof);
}

#[test]
fn unterminated_code_section_immediates_fail_closed() {
    assert_code_framing_error(&[0x80, 0x80, 0x80, 0x80, 0x80], ParseError::InvalidLeb128);
}

#[test]
fn overflowing_code_section_immediates_fail_closed() {
    assert_code_framing_error(&[0x80, 0x80, 0x80, 0x80, 0x10], ParseError::Leb128Overflow);
}

#[test]
fn noncanonical_code_section_immediates_remain_accepted() {
    let section_count = module_with_code_payload(&[0x80, 0x00]);
    assert!(parse_module(&section_count).is_ok());

    let body_length = module_with_code_payload(&[
        0x01, // one body
        0x82, 0x00, // noncanonical body length 2
        0x00, // zero local groups
        0x0b, // end
    ]);
    assert!(parse_module(&body_length).is_ok());

    let local_group_count = module_with_code_payload(&[
        0x01, // one body
        0x03, // body length 3
        0x80, 0x00, // noncanonical zero local groups
        0x0b, // end
    ]);
    assert!(parse_module(&local_group_count).is_ok());

    let local_declaration_count = module_with_code_payload(&[
        0x01, // one body
        0x05, // body length 5
        0x01, // one local group
        0x80, 0x00, // noncanonical zero locals in the group
        0x7f, // i32
        0x0b, // end
    ]);
    assert!(parse_module(&local_declaration_count).is_ok());
}

#[test]
fn noncanonical_positive_code_count_preserves_function_body() {
    let module = module_with_code_payload(&[
        0x81, 0x00, // noncanonical one function body
        0x02, // body length 2
        0x00, // zero local groups
        0x0b, // end
    ]);

    let parsed = parse_module(&module).expect("noncanonical positive code count must parse");
    assert_eq!(parsed.code.len(), 1);
    assert!(parsed.code[0].locals.is_empty());
    assert_eq!(parsed.code[0].code, vec![0x0b]);
}

#[test]
fn noncanonical_positive_local_counts_preserve_declared_locals() {
    let module = module_with_code_payload(&[
        0x01, // one body
        0x06, // body length 6
        0x81, 0x00, // noncanonical one local group
        0x82, 0x00, // noncanonical two locals in the group
        0x7f, // i32
        0x0b, // end
    ]);

    let parsed = parse_module(&module).expect("noncanonical positive local counts must parse");
    assert_eq!(parsed.code.len(), 1);
    assert_eq!(parsed.code[0].locals, vec![(2, ValueType::I32)]);
    assert_eq!(parsed.code[0].code, vec![0x0b]);
}
