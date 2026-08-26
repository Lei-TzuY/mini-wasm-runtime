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

fn module_with_segment_offset(section_id: u8, expr: &[u8]) -> Vec<u8> {
    let mut payload = vec![0x01, 0x00];
    payload.extend_from_slice(expr);

    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    module.push(section_id);
    push_u32(&mut module, payload.len() as u32);
    module.extend_from_slice(&payload);
    module
}

fn assert_both_offsets_fail(expr: &[u8], expected: ParseError) {
    for section_id in [9, 11] {
        assert_eq!(
            parse_module(&module_with_segment_offset(section_id, expr)),
            Err(expected.clone()),
            "section {section_id} must reject a malformed signed offset LEB"
        );
    }
}

#[test]
fn unterminated_signed_offset_immediates_fail_closed() {
    let mut i32_expr = vec![0x41];
    i32_expr.extend([0x80; 5]);
    assert_both_offsets_fail(&i32_expr, ParseError::InvalidLeb128);

    let mut i64_expr = vec![0x42];
    i64_expr.extend([0x80; 10]);
    assert_both_offsets_fail(&i64_expr, ParseError::InvalidLeb128);
}

#[test]
fn overflowing_signed_offset_immediates_fail_closed() {
    assert_both_offsets_fail(
        &[0x41, 0x80, 0x80, 0x80, 0x80, 0x10],
        ParseError::Leb128Overflow,
    );

    let mut i64_expr = vec![0x42];
    i64_expr.extend([0x80; 9]);
    i64_expr.push(0x02);
    assert_both_offsets_fail(&i64_expr, ParseError::Leb128Overflow);
}
