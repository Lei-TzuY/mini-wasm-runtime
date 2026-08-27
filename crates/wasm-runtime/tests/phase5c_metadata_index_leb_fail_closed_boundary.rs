use wasm_parser::{parse_module, ImportDesc, ParseError};

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

fn module_with_section(id: u8, payload: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    module.push(id);
    push_u32(&mut module, payload.len() as u32);
    module.extend_from_slice(payload);
    module
}

fn import_payload(index_bytes: &[u8]) -> Vec<u8> {
    let mut payload = vec![
        0x01, // one import
        0x00, // empty module name
        0x00, // empty field name
        0x00, // function import
    ];
    payload.extend_from_slice(index_bytes);
    payload
}

fn function_payload(index_bytes: &[u8]) -> Vec<u8> {
    let mut payload = vec![0x01]; // one defined function type index
    payload.extend_from_slice(index_bytes);
    payload
}

fn export_payload(index_bytes: &[u8]) -> Vec<u8> {
    let mut payload = vec![
        0x01, // one export
        0x00, // empty export name
        0x00, // function export
    ];
    payload.extend_from_slice(index_bytes);
    payload
}

fn assert_index_error(index_bytes: &[u8], expected: ParseError) {
    assert_eq!(
        parse_module(&module_with_section(2, &import_payload(index_bytes))),
        Err(expected.clone())
    );
    assert_eq!(
        parse_module(&module_with_section(3, &function_payload(index_bytes))),
        Err(expected.clone())
    );
    assert_eq!(
        parse_module(&module_with_section(7, &export_payload(index_bytes))),
        Err(expected)
    );
}

#[test]
fn truncated_metadata_indices_fail_closed() {
    assert_index_error(&[0x80], ParseError::UnexpectedEof);
}

#[test]
fn unterminated_metadata_indices_fail_closed() {
    assert_index_error(&[0x80, 0x80, 0x80, 0x80, 0x80], ParseError::InvalidLeb128);
}

#[test]
fn overflowing_metadata_indices_fail_closed() {
    assert_index_error(&[0x80, 0x80, 0x80, 0x80, 0x10], ParseError::Leb128Overflow);
}

#[test]
fn noncanonical_metadata_indices_remain_accepted() {
    let import = parse_module(&module_with_section(2, &import_payload(&[0x80, 0x00])))
        .expect("noncanonical but width-valid import type index must remain accepted");
    assert_eq!(import.imports[0].desc, ImportDesc::Function(0));

    let function = parse_module(&module_with_section(3, &function_payload(&[0x80, 0x00])))
        .expect("noncanonical but width-valid function type index must remain accepted");
    assert_eq!(function.function_type_indices, vec![0]);

    let export = parse_module(&module_with_section(7, &export_payload(&[0x80, 0x00])))
        .expect("noncanonical but width-valid export index must remain accepted");
    assert_eq!(export.exports[0].index, 0);
}
