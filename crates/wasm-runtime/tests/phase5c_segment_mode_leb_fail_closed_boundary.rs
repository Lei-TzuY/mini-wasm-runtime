use wasm_parser::{parse_module, ParseError};

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

fn module_with_segment_mode(section_id: u8, mode_bytes: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let mut payload = vec![0x01]; // one segment
    payload.extend_from_slice(mode_bytes);
    push_section(&mut module, section_id, &payload);
    module
}

fn assert_both_segment_sections_fail(mode_bytes: &[u8], expected: ParseError) {
    for (kind, section_id) in [("element", 9), ("data", 11)] {
        let module = module_with_segment_mode(section_id, mode_bytes);
        assert_eq!(
            parse_module(&module),
            Err(expected.clone()),
            "unexpected parser result for malformed {kind} segment mode"
        );
    }
}

#[test]
fn truncated_segment_mode_leb_fails_at_discriminant_decode() {
    assert_both_segment_sections_fail(&[0x80], ParseError::UnexpectedEof);
}

#[test]
fn unterminated_five_byte_segment_mode_leb_is_rejected() {
    assert_both_segment_sections_fail(&[0x80, 0x80, 0x80, 0x80, 0x80], ParseError::InvalidLeb128);
}

#[test]
fn overflowing_segment_mode_leb_is_rejected() {
    assert_both_segment_sections_fail(&[0x80, 0x80, 0x80, 0x80, 0x10], ParseError::Leb128Overflow);
}

#[test]
fn canonical_unknown_segment_modes_fail_closed_without_payload_bytes() {
    let cases: &[(u32, &[u8])] = &[
        (8, &[0x08]),
        (128, &[0x80, 0x01]),
        (u32::MAX, &[0xff, 0xff, 0xff, 0xff, 0x0f]),
    ];

    for &(mode, mode_bytes) in cases {
        let element = module_with_segment_mode(9, mode_bytes);
        assert_eq!(
            parse_module(&element),
            Err(ParseError::UnsupportedElementSegmentMode(mode)),
            "canonical unknown element mode {mode} must fail at the discriminant"
        );

        let data = module_with_segment_mode(11, mode_bytes);
        assert_eq!(
            parse_module(&data),
            Err(ParseError::UnsupportedDataSegmentMode(mode)),
            "canonical unknown data mode {mode} must fail at the discriminant"
        );
    }
}

#[test]
fn noncanonical_unknown_segment_modes_decode_then_fail_semantically() {
    let cases: &[(u32, &[u8])] = &[(8, &[0x88, 0x00]), (128, &[0x80, 0x81, 0x00])];

    for &(mode, mode_bytes) in cases {
        let element = module_with_segment_mode(9, mode_bytes);
        assert_eq!(
            parse_module(&element),
            Err(ParseError::UnsupportedElementSegmentMode(mode)),
            "noncanonical element mode {mode} must decode before semantic rejection"
        );

        let data = module_with_segment_mode(11, mode_bytes);
        assert_eq!(
            parse_module(&data),
            Err(ParseError::UnsupportedDataSegmentMode(mode)),
            "noncanonical data mode {mode} must decode before semantic rejection"
        );
    }
}

#[test]
fn noncanonical_supported_mode_two_decodes_before_payload_parsing() {
    let mut data_module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut data_module, 5, &[0x01, 0x00, 0x00]); // one memory, min=0
    push_section(
        &mut data_module,
        11,
        &[
            0x01, // one data segment
            0x82, 0x00, // noncanonical u32 LEB for mode 2
            0x00, // explicit memory index 0
            0x41, 0x00, 0x0b, // i32.const 0; end
            0x00, // empty byte vector
        ],
    );

    let parsed_data = parse_module(&data_module).expect("noncanonical mode-2 data segment must parse");
    assert_eq!(parsed_data.data.len(), 1);
    assert_eq!(parsed_data.data[0].memory_index, 0);
    assert_eq!(parsed_data.data[0].offset, 0);
    assert!(parsed_data.data[0].bytes.is_empty());

    let mut element_module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(
        &mut element_module,
        9,
        &[
            0x01, // one element segment
            0x82, 0x00, // noncanonical u32 LEB for mode 2
            0x00, // explicit table index 0
            0x41, 0x00, 0x0b, // i32.const 0; end
            0x00, // elemkind funcref
            0x00, // empty function-index vector
        ],
    );
    let parsed_element =
        parse_module(&element_module).expect("noncanonical mode-2 element segment must parse");
    assert_eq!(parsed_element.elements.len(), 1);
    assert_eq!(parsed_element.elements[0].table_index, 0);
    assert_eq!(parsed_element.elements[0].offset, 0);
    assert!(parsed_element.elements[0].function_indices.is_empty());
}
