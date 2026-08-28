use wasm_parser::parse_module;

fn module_with_noncanonical_zero_count(section_id: u8) -> Vec<u8> {
    vec![
        0x00, 0x61, 0x73, 0x6d, // magic
        0x01, 0x00, 0x00, 0x00, // version
        section_id,
        0x02, // payload length
        0x80, 0x00, // noncanonical u32 LEB encoding of vector count 0
    ]
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
