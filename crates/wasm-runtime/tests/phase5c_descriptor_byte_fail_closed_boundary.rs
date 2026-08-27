use wasm_parser::{parse_module, ParseError};

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    assert!(payload.len() < 128);
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

fn header() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

#[test]
fn import_kind_byte_is_required_and_invalid_kinds_fail_closed() {
    let mut truncated = header();
    push_section(&mut truncated, 2, &[0x01, 0x00, 0x00]);
    assert_eq!(parse_module(&truncated), Err(ParseError::UnexpectedEof));

    let mut invalid = header();
    push_section(&mut invalid, 2, &[0x01, 0x00, 0x00, 0x04]);
    assert_eq!(
        parse_module(&invalid),
        Err(ParseError::InvalidImportKind(0x04))
    );
}

#[test]
fn export_kind_byte_is_required_and_invalid_kinds_fail_closed() {
    let mut truncated = header();
    push_section(&mut truncated, 7, &[0x01, 0x00]);
    assert_eq!(parse_module(&truncated), Err(ParseError::UnexpectedEof));

    let mut invalid = header();
    push_section(&mut invalid, 7, &[0x01, 0x00, 0x04, 0x00]);
    assert_eq!(
        parse_module(&invalid),
        Err(ParseError::InvalidExportKind(0x04))
    );
}

#[test]
fn defined_global_mutability_byte_is_required_and_invalid_values_fail_closed() {
    let mut truncated = header();
    push_section(&mut truncated, 6, &[0x01, 0x7f]);
    assert_eq!(parse_module(&truncated), Err(ParseError::UnexpectedEof));

    let mut invalid = header();
    push_section(&mut invalid, 6, &[0x01, 0x7f, 0x02]);
    assert_eq!(
        parse_module(&invalid),
        Err(ParseError::InvalidMutability(0x02))
    );
}

#[test]
fn imported_global_mutability_byte_is_required_and_invalid_values_fail_closed() {
    let mut truncated = header();
    push_section(&mut truncated, 2, &[0x01, 0x00, 0x00, 0x03, 0x7f]);
    assert_eq!(parse_module(&truncated), Err(ParseError::UnexpectedEof));

    let mut invalid = header();
    push_section(
        &mut invalid,
        2,
        &[0x01, 0x00, 0x00, 0x03, 0x7f, 0x02],
    );
    assert_eq!(
        parse_module(&invalid),
        Err(ParseError::InvalidMutability(0x02))
    );
}

#[test]
fn defined_memory_limits_flags_are_required_and_invalid_flags_fail_closed() {
    let mut truncated = header();
    push_section(&mut truncated, 5, &[0x01]);
    assert_eq!(parse_module(&truncated), Err(ParseError::UnexpectedEof));

    let mut invalid = header();
    push_section(&mut invalid, 5, &[0x01, 0x02, 0x00]);
    assert_eq!(
        parse_module(&invalid),
        Err(ParseError::InvalidLimitsFlags(0x02))
    );
}

#[test]
fn defined_table_limits_flags_are_required_and_invalid_flags_fail_closed() {
    let mut truncated = header();
    push_section(&mut truncated, 4, &[0x01, 0x70]);
    assert_eq!(parse_module(&truncated), Err(ParseError::UnexpectedEof));

    let mut invalid = header();
    push_section(&mut invalid, 4, &[0x01, 0x70, 0x02, 0x00]);
    assert_eq!(
        parse_module(&invalid),
        Err(ParseError::InvalidLimitsFlags(0x02))
    );
}

#[test]
fn imported_memory_limits_flags_are_required_and_invalid_flags_fail_closed() {
    let mut truncated = header();
    push_section(&mut truncated, 2, &[0x01, 0x00, 0x00, 0x02]);
    assert_eq!(parse_module(&truncated), Err(ParseError::UnexpectedEof));

    let mut invalid = header();
    push_section(&mut invalid, 2, &[0x01, 0x00, 0x00, 0x02, 0x02, 0x00]);
    assert_eq!(
        parse_module(&invalid),
        Err(ParseError::InvalidLimitsFlags(0x02))
    );
}

#[test]
fn imported_table_limits_flags_are_required_and_invalid_flags_fail_closed() {
    let mut truncated = header();
    push_section(&mut truncated, 2, &[0x01, 0x00, 0x00, 0x01, 0x70]);
    assert_eq!(parse_module(&truncated), Err(ParseError::UnexpectedEof));

    let mut invalid = header();
    push_section(
        &mut invalid,
        2,
        &[0x01, 0x00, 0x00, 0x01, 0x70, 0x02, 0x00],
    );
    assert_eq!(
        parse_module(&invalid),
        Err(ParseError::InvalidLimitsFlags(0x02))
    );
}
