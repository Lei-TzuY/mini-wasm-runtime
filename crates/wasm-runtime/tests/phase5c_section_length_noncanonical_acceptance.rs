use wasm_parser::parse_module;

fn module_with_noncanonical_one_byte_section(section_id: u8, payload: u8) -> Vec<u8> {
    let mut module = vec![
        0x00, 0x61, 0x73, 0x6d, // magic
        0x01, 0x00, 0x00, 0x00, // version
        section_id, 0x81, 0x00, // noncanonical u32 LEB encoding of payload length 1
        payload,
    ];
    module.shrink_to_fit();
    module
}

#[test]
fn noncanonical_standard_section_lengths_remain_accepted() {
    // For vector sections, a one-byte payload containing count=0 is a complete section.
    for section_id in [1, 2, 3, 4, 5, 6, 7, 9, 10, 11] {
        let module = module_with_noncanonical_one_byte_section(section_id, 0x00);
        parse_module(&module).unwrap_or_else(|error| {
            panic!("section {section_id} must accept a width-valid noncanonical length: {error:?}")
        });
    }

    // Start is not a vector section; its one-byte payload is function index 0.
    let module = module_with_noncanonical_one_byte_section(8, 0x00);
    let parsed = parse_module(&module)
        .expect("start section must accept a width-valid noncanonical payload length");
    assert_eq!(parsed.start, Some(0));
}

#[test]
fn noncanonical_custom_section_length_remains_accepted_with_payload() {
    let module = module_with_noncanonical_one_byte_section(0, 0xaa);
    parse_module(&module)
        .expect("custom sections must accept width-valid noncanonical payload lengths");
}
