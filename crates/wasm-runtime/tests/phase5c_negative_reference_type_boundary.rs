use wasm_parser::{parse_module, ParseError};

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    assert!(payload.len() < 128);
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

fn module_header() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

#[test]
fn function_parameter_funcref_remains_fail_closed_at_parse_boundary() {
    let mut bytes = module_header();
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x01, 0x70, 0x00]);

    assert_eq!(
        parse_module(&bytes),
        Err(ParseError::UnsupportedValueType(0x70))
    );
}

#[test]
fn function_result_externref_remains_fail_closed_at_parse_boundary() {
    let mut bytes = module_header();
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x00, 0x01, 0x6f]);

    assert_eq!(
        parse_module(&bytes),
        Err(ParseError::UnsupportedValueType(0x6f))
    );
}

#[test]
fn defined_global_funcref_remains_fail_closed_before_const_expr_admission() {
    let mut bytes = module_header();
    push_section(&mut bytes, 6, &[0x01, 0x70, 0x00, 0xd0, 0x70, 0x0b]);

    assert_eq!(
        parse_module(&bytes),
        Err(ParseError::UnsupportedValueType(0x70))
    );
}

#[test]
fn imported_reference_globals_remain_fail_closed_before_binding_admission() {
    for unsupported in [0x70, 0x6f] {
        let mut bytes = module_header();
        let import = [
            0x01, // one import
            0x03, b'e', b'n', b'v', // module name
            0x01, b'g', // field name
            0x03, // global import
            unsupported,
            0x00, // immutable
        ];
        push_section(&mut bytes, 2, &import);

        assert_eq!(
            parse_module(&bytes),
            Err(ParseError::UnsupportedValueType(unsupported))
        );
    }
}

#[test]
fn mutable_imported_reference_globals_remain_fail_closed_before_binding_admission() {
    for unsupported in [0x70, 0x6f] {
        let mut bytes = module_header();
        let import = [
            0x01, // one import
            0x03, b'e', b'n', b'v', // module name
            0x01, b'g', // field name
            0x03, // global import
            unsupported,
            0x01, // mutable
        ];
        push_section(&mut bytes, 2, &import);

        assert_eq!(
            parse_module(&bytes),
            Err(ParseError::UnsupportedValueType(unsupported))
        );
    }
}

#[test]
fn defined_externref_table_remains_fail_closed_at_reference_type_boundary() {
    let mut bytes = module_header();
    push_section(&mut bytes, 4, &[0x01, 0x6f, 0x00, 0x01]);

    assert_eq!(
        parse_module(&bytes),
        Err(ParseError::InvalidReferenceType(0x6f))
    );
}

#[test]
fn imported_externref_table_remains_fail_closed_at_reference_type_boundary() {
    let mut bytes = module_header();
    let import = [
        0x01, // one import
        0x03, b'e', b'n', b'v', // module name
        0x01, b't', // field name
        0x01, // table import
        0x6f, // externref: valid WebAssembly reference type, unsupported here
        0x00, 0x01, // limits: min 1, no max
    ];
    push_section(&mut bytes, 2, &import);

    assert_eq!(
        parse_module(&bytes),
        Err(ParseError::InvalidReferenceType(0x6f))
    );
}