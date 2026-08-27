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

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn module_with_global(value_type: u8, expr: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let mut payload = vec![0x01, value_type, 0x00];
    payload.extend_from_slice(expr);
    push_section(&mut module, 6, &payload);
    module
}

fn module_with_segment_offset(section_id: u8, expr: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let mut payload = vec![0x01, 0x00];
    payload.extend_from_slice(expr);
    push_section(&mut module, section_id, &payload);
    module
}

#[test]
fn defined_global_constant_expression_terminators_fail_closed() {
    let literals: &[(u8, &[u8])] = &[
        (0x7f, &[0x41, 0x00]),
        (0x7e, &[0x42, 0x00]),
        (0x7d, &[0x43, 0x00, 0x00, 0x00, 0x00]),
        (
            0x7c,
            &[0x44, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        ),
    ];

    for &(value_type, literal) in literals {
        assert_eq!(
            parse_module(&module_with_global(value_type, literal)),
            Err(ParseError::UnexpectedEof),
            "a defined-global constant expression missing its end byte must fail at EOF"
        );

        let mut wrong_end = literal.to_vec();
        wrong_end.push(0x01);
        assert_eq!(
            parse_module(&module_with_global(value_type, &wrong_end)),
            Err(ParseError::ConstExprMissingEnd),
            "a defined-global constant expression with a non-end terminator must fail closed"
        );
    }
}

#[test]
fn active_segment_offset_terminators_fail_closed() {
    let literal = [0x41, 0x00];
    let wrong_end = [0x41, 0x00, 0x01];

    for section_id in [9, 11] {
        assert_eq!(
            parse_module(&module_with_segment_offset(section_id, &literal)),
            Err(ParseError::UnexpectedEof),
            "section {section_id} offset expression missing its end byte must fail at EOF"
        );
        assert_eq!(
            parse_module(&module_with_segment_offset(section_id, &wrong_end)),
            Err(ParseError::ConstExprMissingEnd),
            "section {section_id} offset expression with a non-end terminator must fail closed"
        );
    }
}
