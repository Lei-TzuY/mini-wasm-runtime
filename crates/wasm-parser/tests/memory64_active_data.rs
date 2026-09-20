use wasm_parser::{parse_module, DataMode, ParseError, ValueType};

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    assert!(payload.len() < 128);
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

fn module_with_active_data(memory_flags: u8, offset_expr: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    // One memory with minimum one page. memory64 uses limits flag 0x04.
    push_section(&mut module, 5, &[0x01, memory_flags, 0x01]);

    let mut data = vec![0x01, 0x00]; // one active segment targeting memory 0
    data.extend_from_slice(offset_expr);
    data.extend_from_slice(&[0x0b, 0x01, b'x']);
    push_section(&mut module, 11, &data);
    module
}

#[test]
fn memory64_active_data_preserves_full_i64_offset_bits() {
    let module = module_with_active_data(
        0x04,
        &[0x42, 0x80, 0x80, 0x80, 0x80, 0x10], // i64.const 0x1_0000_0000
    );
    let parsed = parse_module(&module).expect("memory64 active data must parse");
    assert_eq!(
        parsed.data[0].mode,
        DataMode::Active {
            memory_index: 0,
            offset: 0x1_0000_0000,
        }
    );
}

#[test]
fn memory32_active_data_keeps_unsigned_i32_address_bits() {
    let module = module_with_active_data(0x00, &[0x41, 0x7f]); // i32.const -1
    let parsed = parse_module(&module).expect("memory32 active data must parse");
    assert_eq!(
        parsed.data[0].mode,
        DataMode::Active {
            memory_index: 0,
            offset: 0xffff_ffff,
        }
    );
}

#[test]
fn active_data_offset_type_follows_target_memory_width() {
    let module = module_with_active_data(0x04, &[0x41, 0x00]); // i32.const 0
    assert_eq!(
        parse_module(&module),
        Err(ParseError::ConstExprTypeMismatch {
            expected: ValueType::I64,
            actual: ValueType::I32,
        })
    );
}
