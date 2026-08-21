use wasm_parser::{parse_module, ImportDesc, ImportKind, ValueType};
use wasm_runtime::{Instance, RuntimeError};
use wasm_validator::{validate, ValidationError};

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

fn push_name(bytes: &mut Vec<u8>, name: &str) {
    push_u32(bytes, name.len() as u32);
    bytes.extend_from_slice(name.as_bytes());
}

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn module_header() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

fn push_function_type(payload: &mut Vec<u8>, params: &[u8], results: &[u8]) {
    payload.push(0x60);
    push_u32(payload, params.len() as u32);
    payload.extend_from_slice(params);
    push_u32(payload, results.len() as u32);
    payload.extend_from_slice(results);
}

fn push_import_prefix(payload: &mut Vec<u8>, module: &str, name: &str, kind: u8) {
    push_name(payload, module);
    push_name(payload, name);
    payload.push(kind);
}

fn mixed_memory_function_import_module() -> Vec<u8> {
    let mut module = module_header();

    let mut types = vec![0x02];
    push_function_type(&mut types, &[], &[0x7f]);
    push_function_type(&mut types, &[], &[0x7f]);
    push_section(&mut module, 1, &types);

    let mut imports = vec![0x02];
    push_import_prefix(&mut imports, "env", "mem", 0x02);
    imports.extend([0x00, 0x01]); // memory min=1, no max
    push_import_prefix(&mut imports, "env", "host", 0x00);
    imports.push(0x00); // function type 0
    push_section(&mut module, 2, &imports);

    push_section(&mut module, 3, &[0x01, 0x01]); // one defined function, type 1
    push_section(
        &mut module,
        7,
        &[
            0x02, 0x03, b'r', b'u', b'n', 0x00, 0x01, // defined function index is 1
            0x03, b'm', b'e', b'm', 0x02, 0x00, // imported memory index is 0
        ],
    );
    push_section(&mut module, 10, &[0x01, 0x04, 0x00, 0x41, 0x2a, 0x0b]);
    module
}

#[test]
fn object_import_does_not_shift_function_index_space() {
    let bytes = mixed_memory_function_import_module();
    let module = parse_module(&bytes).expect("mixed imports parse");
    assert_eq!(module.imports.len(), 2);
    assert_eq!(module.function_import_count(), 1);
    assert_eq!(module.memory_import_count(), 1);
    assert_eq!(module.function_count(), 2);
    assert_eq!(module.memory_count(), 1);
    assert_eq!(module.function_import(0).unwrap().name, "host");
    assert!(matches!(module.imports[0].desc, ImportDesc::Memory(_)));
    assert!(matches!(module.imports[1].desc, ImportDesc::Function(0)));
    assert_eq!(validate(&module), Ok(()));
}

#[test]
fn imported_memory_is_visible_to_export_validation_but_runtime_fails_closed() {
    let module = parse_module(&mixed_memory_function_import_module()).unwrap();
    assert_eq!(validate(&module), Ok(()));
    let error = Instance::new(module).expect_err("object import must not be copied implicitly");
    assert!(matches!(
        error,
        RuntimeError::UnsupportedObjectImport {
            kind: ImportKind::Memory,
            ..
        }
    ));
}

#[test]
fn imported_table_is_visible_to_element_and_export_index_spaces() {
    let mut module = module_header();
    let mut types = vec![0x01];
    push_function_type(&mut types, &[], &[]);
    push_section(&mut module, 1, &types);

    let mut imports = vec![0x01];
    push_import_prefix(&mut imports, "env", "tab", 0x01);
    imports.extend([0x70, 0x00, 0x01]); // funcref, min=1
    push_section(&mut module, 2, &imports);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 7, &[0x01, 0x03, b't', b'a', b'b', 0x01, 0x00]);
    push_section(&mut module, 9, &[0x01, 0x00, 0x41, 0x00, 0x0b, 0x01, 0x00]);
    push_section(&mut module, 10, &[0x01, 0x02, 0x00, 0x0b]);

    let parsed = parse_module(&module).unwrap();
    assert_eq!(parsed.table_count(), 1);
    assert_eq!(validate(&parsed), Ok(()));
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::UnsupportedObjectImport {
            kind: ImportKind::Table,
            ..
        })
    ));
}

#[test]
fn imported_global_participates_in_global_get_typing() {
    let mut module = module_header();
    let mut types = vec![0x01];
    push_function_type(&mut types, &[], &[0x7e]);
    push_section(&mut module, 1, &types);

    let mut imports = vec![0x01];
    push_import_prefix(&mut imports, "env", "g", 0x03);
    imports.extend([0x7e, 0x00]); // immutable i64 global
    push_section(&mut module, 2, &imports);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 7, &[0x01, 0x01, b'g', 0x03, 0x00]);
    push_section(&mut module, 10, &[0x01, 0x04, 0x00, 0x23, 0x00, 0x0b]);

    let parsed = parse_module(&module).unwrap();
    assert_eq!(parsed.global_count(), 1);
    assert_eq!(parsed.global_type(0).unwrap().value_type, ValueType::I64);
    assert_eq!(validate(&parsed), Ok(()));
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::UnsupportedObjectImport {
            kind: ImportKind::Global,
            ..
        })
    ));
}

#[test]
fn imported_and_defined_memory_still_obey_single_memory_runtime_subset() {
    let mut module = module_header();
    let mut imports = vec![0x01];
    push_import_prefix(&mut imports, "env", "mem", 0x02);
    imports.extend([0x00, 0x01]);
    push_section(&mut module, 2, &imports);
    push_section(&mut module, 5, &[0x01, 0x00, 0x01]);

    let parsed = parse_module(&module).unwrap();
    assert_eq!(parsed.memory_count(), 2);
    assert_eq!(
        validate(&parsed),
        Err(ValidationError::UnsupportedMemoryCount { count: 2 })
    );
}

#[test]
fn imported_memory_limits_are_validated_before_runtime_rejection() {
    let mut module = module_header();
    let mut imports = vec![0x01];
    push_import_prefix(&mut imports, "env", "mem", 0x02);
    imports.extend([0x01, 0x02, 0x01]); // min=2, max=1
    push_section(&mut module, 2, &imports);

    let parsed = parse_module(&module).unwrap();
    assert_eq!(
        validate(&parsed),
        Err(ValidationError::InvalidMemoryLimits {
            memory: 0,
            min: 2,
            max: 1,
        })
    );
}
