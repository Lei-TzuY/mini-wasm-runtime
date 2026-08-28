use wasm_parser::{parse_module, ParseError};

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

fn module_header() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

fn push_custom(module: &mut Vec<u8>, name: u8) {
    push_section(module, 0, &[0x01, name]);
}

#[test]
fn repeated_custom_sections_are_allowed_around_standard_sections() {
    let mut module = module_header();
    push_custom(&mut module, b'a');
    push_section(&mut module, 1, &[0x00]); // empty type vector
    push_custom(&mut module, b'b');
    push_section(&mut module, 2, &[0x00]); // empty import vector
    push_custom(&mut module, b'c');
    push_section(&mut module, 3, &[0x00]); // empty function vector
    push_custom(&mut module, b'd');

    let parsed =
        parse_module(&module).expect("custom sections may repeat between standard sections");
    assert!(parsed.types.is_empty());
    assert!(parsed.imports.is_empty());
    assert!(parsed.function_type_indices.is_empty());
}

#[test]
fn custom_section_does_not_hide_standard_section_ordering_error() {
    let mut module = module_header();
    push_section(&mut module, 3, &[0x00]); // function section
    push_custom(&mut module, b'x');
    push_section(&mut module, 2, &[0x00]); // import section is now out of order

    assert_eq!(
        parse_module(&module),
        Err(ParseError::SectionOutOfOrder {
            previous: 3,
            current: 2,
        })
    );
}

#[test]
fn custom_section_does_not_hide_duplicate_standard_section() {
    let mut module = module_header();
    push_section(&mut module, 1, &[0x00]);
    push_custom(&mut module, b'x');
    push_section(&mut module, 1, &[0x00]);

    assert_eq!(parse_module(&module), Err(ParseError::DuplicateSection(1)));
}
