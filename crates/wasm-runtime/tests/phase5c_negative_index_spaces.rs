use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError};
use wasm_validator::ValidationError;

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

fn header() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

#[test]
fn memory_export_cannot_escape_memory_index_space() {
    let mut module = header();
    push_section(&mut module, 5, &[0x01, 0x00, 0x01]);
    push_section(
        &mut module,
        7,
        &[0x01, 0x03, b'm', b'e', b'm', 0x02, 0x01],
    );

    let error = Instance::new(parse_module(&module).unwrap())
        .expect_err("memory export must remain inside the memory index space");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::MemoryExportOutOfBounds {
            memory_index: 1,
            ..
        })
    ));
}

#[test]
fn table_export_cannot_escape_table_index_space() {
    let mut module = header();
    push_section(&mut module, 4, &[0x01, 0x70, 0x00, 0x01]);
    push_section(
        &mut module,
        7,
        &[0x01, 0x03, b't', b'a', b'b', 0x01, 0x01],
    );

    let error = Instance::new(parse_module(&module).unwrap())
        .expect_err("table export must remain inside the table index space");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::TableExportOutOfBounds {
            table_index: 1,
            ..
        })
    ));
}

#[test]
fn global_export_cannot_escape_global_index_space() {
    let mut module = header();
    push_section(&mut module, 6, &[0x01, 0x7f, 0x00, 0x41, 0x00, 0x0b]);
    push_section(&mut module, 7, &[0x01, 0x01, b'g', 0x03, 0x01]);

    let error = Instance::new(parse_module(&module).unwrap())
        .expect_err("global export must remain inside the global index space");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::GlobalExportOutOfBounds {
            global_index: 1,
            ..
        })
    ));
}

#[test]
fn table_with_minimum_above_maximum_is_rejected_before_instantiation() {
    let mut module = header();
    push_section(&mut module, 4, &[0x01, 0x70, 0x01, 0x02, 0x01]);

    let error = Instance::new(parse_module(&module).unwrap())
        .expect_err("invalid table limits must fail closed");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::InvalidTableLimits {
            table: 0,
            min: 2,
            max: 1,
        })
    ));
}
