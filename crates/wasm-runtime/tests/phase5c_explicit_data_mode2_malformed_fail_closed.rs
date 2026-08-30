use wasm_parser::{parse_module, ParseError, ValueType};

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

fn module_with_data_payload(payload: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut module, 11, payload);
    module
}

#[test]
fn explicit_data_mode_two_requires_memory_index_immediate() {
    let module = module_with_data_payload(&[
        0x01, // one data segment
        0x02, // active mode with explicit memory index
    ]);

    assert_eq!(parse_module(&module), Err(ParseError::UnexpectedEof));
}

#[test]
fn explicit_data_mode_two_rejects_unterminated_memory_index_leb() {
    let module = module_with_data_payload(&[
        0x01, // one data segment
        0x02, // active mode with explicit memory index
        0x80, 0x80, 0x80, 0x80, 0x80, // unterminated five-byte u32 LEB
    ]);

    assert_eq!(parse_module(&module), Err(ParseError::InvalidLeb128));
}

#[test]
fn explicit_data_mode_two_rejects_overflowing_memory_index_leb() {
    let module = module_with_data_payload(&[
        0x01, // one data segment
        0x02, // active mode with explicit memory index
        0xff, 0xff, 0xff, 0xff, 0x10, // payload exceeds u32 on byte five
    ]);

    assert_eq!(parse_module(&module), Err(ParseError::Leb128Overflow));
}

#[test]
fn explicit_data_mode_two_preserves_i32_offset_type_requirement() {
    let module = module_with_data_payload(&[
        0x01, // one data segment
        0x02, // active mode with explicit memory index
        0x00, // memory index 0
        0x42, 0x00, 0x0b, // i64.const 0; end
        0x00, // empty byte vector (must not be reached)
    ]);

    assert_eq!(
        parse_module(&module),
        Err(ParseError::ConstExprTypeMismatch {
            expected: ValueType::I32,
            actual: ValueType::I64,
        })
    );
}

#[test]
fn explicit_data_mode_two_rejects_truncated_byte_vector_transactionally_at_parse_time() {
    let module = module_with_data_payload(&[
        0x01, // one data segment
        0x02, // active mode with explicit memory index
        0x00, // memory index 0
        0x41, 0x00, 0x0b, // i32.const 0; end
        0x01, // one byte declared, but payload is absent
    ]);

    assert_eq!(parse_module(&module), Err(ParseError::UnexpectedEof));
}
