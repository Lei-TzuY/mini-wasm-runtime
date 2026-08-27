use wasm_parser::{parse_module, ExportKind, ImportDesc, ParseError};

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

fn module_with_section(id: u8, payload: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    module.push(id);
    push_u32(&mut module, payload.len() as u32);
    module.extend_from_slice(payload);
    module
}

fn assert_import_module_name_error(encoded_name: &[u8], expected: ParseError) {
    let mut payload = vec![0x01];
    payload.extend_from_slice(encoded_name);
    assert_eq!(
        parse_module(&module_with_section(2, &payload)),
        Err(expected)
    );
}

fn assert_import_field_name_error(encoded_name: &[u8], expected: ParseError) {
    let mut payload = vec![0x01, 0x00];
    payload.extend_from_slice(encoded_name);
    assert_eq!(
        parse_module(&module_with_section(2, &payload)),
        Err(expected)
    );
}

fn assert_export_name_error(encoded_name: &[u8], expected: ParseError) {
    let mut payload = vec![0x01];
    payload.extend_from_slice(encoded_name);
    assert_eq!(
        parse_module(&module_with_section(7, &payload)),
        Err(expected)
    );
}

fn assert_all_name_positions(encoded_name: &[u8], expected: ParseError) {
    assert_import_module_name_error(encoded_name, expected.clone());
    assert_import_field_name_error(encoded_name, expected.clone());
    assert_export_name_error(encoded_name, expected);
}

#[test]
fn truncated_name_lengths_fail_closed() {
    assert_all_name_positions(TRUNCATED_U32, ParseError::UnexpectedEof);
}

#[test]
fn unterminated_name_lengths_fail_closed() {
    assert_all_name_positions(UNTERMINATED_U32, ParseError::InvalidLeb128);
}

#[test]
fn overflowing_name_lengths_fail_closed() {
    assert_all_name_positions(OVERFLOWING_U32, ParseError::Leb128Overflow);
}

#[test]
fn short_name_payloads_fail_closed() {
    assert_all_name_positions(&[0x02, b'a'], ParseError::UnexpectedEof);
}

#[test]
fn invalid_utf8_names_fail_closed() {
    assert_all_name_positions(&[0x01, 0xff], ParseError::InvalidUtf8);
}

#[test]
fn noncanonical_name_lengths_remain_accepted() {
    let import_payload = [
        0x01, // one import
        0x81, 0x00, b'm', // module name "m" with noncanonical length 1
        0x00, // empty field name
        0x00, 0x00, // function import, type index 0
    ];
    let import_module = parse_module(&module_with_section(2, &import_payload))
        .expect("width-valid noncanonical import name length must remain accepted");
    assert_eq!(import_module.imports.len(), 1);
    assert_eq!(import_module.imports[0].module, "m");
    assert!(matches!(
        import_module.imports[0].desc,
        ImportDesc::Function(0)
    ));

    let export_payload = [
        0x01, // one export
        0x81, 0x00, b'x', // export name "x" with noncanonical length 1
        0x00, 0x00, // function export, index 0
    ];
    let export_module = parse_module(&module_with_section(7, &export_payload))
        .expect("width-valid noncanonical export name length must remain accepted");
    assert_eq!(export_module.exports.len(), 1);
    assert_eq!(export_module.exports[0].name, "x");
    assert_eq!(export_module.exports[0].kind, ExportKind::Function);
    assert_eq!(export_module.exports[0].index, 0);
}
