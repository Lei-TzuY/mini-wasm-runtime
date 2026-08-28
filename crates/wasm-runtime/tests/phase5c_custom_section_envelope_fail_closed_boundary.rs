use wasm_parser::{parse_module, ParseError};

fn module_with_tail(tail: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    module.extend_from_slice(tail);
    module
}

#[test]
fn malformed_custom_section_length_leb_fails_closed() {
    let cases: &[(&[u8], ParseError)] = &[
        (&[0x00, 0x80], ParseError::UnexpectedEof),
        (
            &[0x00, 0x80, 0x80, 0x80, 0x80, 0x80],
            ParseError::InvalidLeb128,
        ),
        (
            &[0x00, 0xff, 0xff, 0xff, 0xff, 0x10],
            ParseError::Leb128Overflow,
        ),
    ];

    for (tail, expected) in cases {
        assert_eq!(parse_module(&module_with_tail(tail)), Err(expected.clone()));
    }
}

#[test]
fn truncated_custom_section_payload_fails_closed() {
    assert_eq!(
        parse_module(&module_with_tail(&[
            0x00, // custom section id
            0x02, // declared payload length
            0x00, // only one payload byte is present
        ])),
        Err(ParseError::UnexpectedEof)
    );
}

#[test]
fn opaque_custom_payload_is_bounded_by_declared_length() {
    let module = module_with_tail(&[
        0x00, // custom section id
        0x03, // payload length
        0x00, 0xff, 0xff, // opaque custom payload
        0x01, // type section id
        0x01, // type section payload length
        0x00, // empty type vector
    ]);

    let parsed = parse_module(&module)
        .expect("opaque custom payload must not consume bytes beyond its declared section length");
    assert!(parsed.types.is_empty());
}
