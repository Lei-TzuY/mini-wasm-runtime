use wasm_parser::parse_module;

fn module_header() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

#[test]
fn noncanonical_custom_section_length_keeps_opaque_payload_bounded() {
    let mut module = module_header();

    module.extend_from_slice(&[
        0x00, // custom section id
        0x86, 0x00, // noncanonical u32 LEB for payload length 6
        0x01, b'x', // valid one-byte custom-section name
        0x01, 0x00, 0x02, 0x00, // opaque bytes resembling standard section framing
    ]);
    module.extend_from_slice(&[
        0x01, // real type section id
        0x01, // payload length
        0x00, // empty type vector
        0x02, // real import section id
        0x01, // payload length
        0x00, // empty import vector
    ]);

    let parsed = parse_module(&module).expect(
        "a width-valid noncanonical custom-section length must not let opaque payload bytes escape",
    );
    assert!(parsed.types.is_empty());
    assert!(parsed.imports.is_empty());
}

#[test]
fn repeated_noncanonical_custom_sections_do_not_reset_standard_ordering() {
    let mut module = module_header();

    module.extend_from_slice(&[
        0x03, 0x01, 0x00, // empty function section
        0x00, 0x82, 0x00, 0x01, b'a', // custom section with noncanonical length 2
        0x00, 0x82, 0x00, 0x01, b'b', // another custom section
        0x02, 0x01, 0x00, // out-of-order empty import section
    ]);

    assert!(matches!(
        parse_module(&module),
        Err(wasm_parser::ParseError::SectionOutOfOrder {
            previous: 3,
            current: 2,
        })
    ));
}
