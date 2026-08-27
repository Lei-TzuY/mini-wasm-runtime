use wasm_parser::{parse_module, ImportDesc, Limits, ParseError};

const TRUNCATED_U32: &[u8] = &[0x80];
const UNTERMINATED_U32: &[u8] = &[0x80, 0x80, 0x80, 0x80, 0x80];
const OVERFLOWING_U32: &[u8] = &[0x80, 0x80, 0x80, 0x80, 0x10];

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

fn import_module(kind: u8, descriptor: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let mut imports = vec![
        0x01, // one import
        0x01, b'm', // module name
        0x01, b'x', // field name
        kind,
    ];
    imports.extend_from_slice(descriptor);
    push_section(&mut module, 2, &imports);
    module
}

fn table_min_descriptor(encoded: &[u8]) -> Vec<u8> {
    let mut descriptor = vec![0x70, 0x00];
    descriptor.extend_from_slice(encoded);
    descriptor
}

fn table_max_descriptor(encoded: &[u8]) -> Vec<u8> {
    let mut descriptor = vec![0x70, 0x01, 0x00];
    descriptor.extend_from_slice(encoded);
    descriptor
}

fn memory_min_descriptor(encoded: &[u8]) -> Vec<u8> {
    let mut descriptor = vec![0x00];
    descriptor.extend_from_slice(encoded);
    descriptor
}

fn memory_max_descriptor(encoded: &[u8]) -> Vec<u8> {
    let mut descriptor = vec![0x01, 0x00];
    descriptor.extend_from_slice(encoded);
    descriptor
}

fn assert_import_error(kind: u8, descriptor: &[u8], expected: ParseError) {
    assert_eq!(
        parse_module(&import_module(kind, descriptor)),
        Err(expected)
    );
}

fn assert_min_error(encoded: &[u8], expected: ParseError) {
    assert_import_error(0x01, &table_min_descriptor(encoded), expected.clone());
    assert_import_error(0x02, &memory_min_descriptor(encoded), expected);
}

fn assert_max_error(encoded: &[u8], expected: ParseError) {
    assert_import_error(0x01, &table_max_descriptor(encoded), expected.clone());
    assert_import_error(0x02, &memory_max_descriptor(encoded), expected);
}

#[test]
fn imported_limit_minimums_fail_closed_on_malformed_leb() {
    assert_min_error(TRUNCATED_U32, ParseError::UnexpectedEof);
    assert_min_error(UNTERMINATED_U32, ParseError::InvalidLeb128);
    assert_min_error(OVERFLOWING_U32, ParseError::Leb128Overflow);
}

#[test]
fn imported_limit_maximums_fail_closed_on_malformed_leb() {
    assert_max_error(TRUNCATED_U32, ParseError::UnexpectedEof);
    assert_max_error(UNTERMINATED_U32, ParseError::InvalidLeb128);
    assert_max_error(OVERFLOWING_U32, ParseError::Leb128Overflow);
}

#[test]
fn imported_limits_accept_noncanonical_width_valid_leb() {
    let table = parse_module(&import_module(0x01, &[0x70, 0x01, 0x81, 0x00, 0x82, 0x00]))
        .expect("noncanonical table limits remain valid");
    assert!(matches!(
        table.imports[0].desc,
        ImportDesc::Table(ty)
            if ty.limits == Limits {
                min: 1,
                max: Some(2),
            }
    ));

    let memory = parse_module(&import_module(0x02, &[0x01, 0x81, 0x00, 0x82, 0x00]))
        .expect("noncanonical memory limits remain valid");
    assert!(matches!(
        memory.imports[0].desc,
        ImportDesc::Memory(ty)
            if ty.limits == Limits {
                min: 1,
                max: Some(2),
            }
    ));
}
