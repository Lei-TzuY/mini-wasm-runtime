use wasm_parser::{parse_module, ExportKind, ImportDesc, ValueType};

fn module_with_section(section_id: u8, payload: &[u8]) -> Vec<u8> {
    let mut module = vec![
        0x00, 0x61, 0x73, 0x6d, // magic
        0x01, 0x00, 0x00, 0x00, // version
        section_id,
        payload.len() as u8,
    ];
    module.extend_from_slice(payload);
    module
}

fn module_with_noncanonical_zero_count(section_id: u8) -> Vec<u8> {
    module_with_section(
        section_id,
        &[
            0x80, 0x00, // noncanonical u32 LEB encoding of vector count 0
        ],
    )
}

#[test]
fn noncanonical_zero_standard_section_counts_remain_accepted() {
    for section_id in 1..=7 {
        let module = module_with_noncanonical_zero_count(section_id);
        parse_module(&module).unwrap_or_else(|error| {
            panic!(
                "section {section_id} must accept a width-valid noncanonical zero vector count: {error:?}"
            )
        });
    }
}

#[test]
fn noncanonical_positive_standard_section_counts_preserve_entries() {
    let type_section = parse_module(&module_with_section(
        1,
        &[
            0x81, 0x00, // one type, noncanonical
            0x60, 0x00, 0x00, // () -> ()
        ],
    ))
    .expect("noncanonical positive type count must remain accepted");
    assert_eq!(type_section.types.len(), 1);
    assert!(type_section.types[0].params.is_empty());
    assert!(type_section.types[0].results.is_empty());

    let import_section = parse_module(&module_with_section(
        2,
        &[
            0x81, 0x00, // one import, noncanonical
            0x00, // empty module name
            0x00, // empty field name
            0x00, 0x00, // function import, type index 0
        ],
    ))
    .expect("noncanonical positive import count must remain accepted");
    assert_eq!(import_section.imports.len(), 1);
    assert!(matches!(
        import_section.imports[0].desc,
        ImportDesc::Function(0)
    ));

    let function_section = parse_module(&module_with_section(
        3,
        &[
            0x81, 0x00, // one function, noncanonical
            0x00, // type index 0
        ],
    ))
    .expect("noncanonical positive function count must remain accepted");
    assert_eq!(function_section.function_type_indices, vec![0]);

    let table_section = parse_module(&module_with_section(
        4,
        &[
            0x81, 0x00, // one table, noncanonical
            0x70, // funcref
            0x00, 0x00, // min-only limits, min 0
        ],
    ))
    .expect("noncanonical positive table count must remain accepted");
    assert_eq!(table_section.tables.len(), 1);
    assert_eq!(table_section.tables[0].limits.min, 0);
    assert_eq!(table_section.tables[0].limits.max, None);

    let memory_section = parse_module(&module_with_section(
        5,
        &[
            0x81, 0x00, // one memory, noncanonical
            0x00, 0x00, // min-only limits, min 0
        ],
    ))
    .expect("noncanonical positive memory count must remain accepted");
    assert_eq!(memory_section.memories.len(), 1);
    assert_eq!(memory_section.memories[0].limits.min, 0);
    assert_eq!(memory_section.memories[0].limits.max, None);

    let global_section = parse_module(&module_with_section(
        6,
        &[
            0x81, 0x00, // one global, noncanonical
            0x7f, 0x00, // immutable i32
            0x41, 0x00, 0x0b, // i32.const 0; end
        ],
    ))
    .expect("noncanonical positive global count must remain accepted");
    assert_eq!(global_section.globals.len(), 1);
    assert_eq!(global_section.globals[0].ty.value_type, ValueType::I32);
    assert!(!global_section.globals[0].ty.mutable);

    let export_section = parse_module(&module_with_section(
        7,
        &[
            0x81, 0x00, // one export, noncanonical
            0x00, // empty export name
            0x00, 0x00, // function export, index 0
        ],
    ))
    .expect("noncanonical positive export count must remain accepted");
    assert_eq!(export_section.exports.len(), 1);
    assert_eq!(export_section.exports[0].kind, ExportKind::Function);
    assert_eq!(export_section.exports[0].index, 0);
}
