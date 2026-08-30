use wasm_parser::{parse_module, ParseError};

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

fn module_with_element_payload(payload: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut module, 9, payload);
    module
}

#[test]
fn explicit_element_mode_two_remains_atomic_fail_closed_before_target_decode() {
    let cases: &[&[u8]] = &[
        &[
            0x01, // one element segment
            0x02, // active mode with explicit table index; no payload follows
        ],
        &[
            0x01, // one element segment
            0x02, // active mode with explicit table index
            0x80, 0x80, 0x80, 0x80, 0x80, // malformed table-index LEB
        ],
        &[
            0x01, // one element segment
            0x02, // active mode with explicit table index
            0x00, // table index 0
            0x42, 0x00, 0x0b, // wrong-type i64 offset expression
            0x00, // elemkind funcref
            0x00, // empty function-index vector
        ],
        &[
            0x01, // one element segment
            0x02, // active mode with explicit table index
            0x00, // table index 0
            0x41, 0x00, 0x0b, // i32.const 0; end
            0x00, // elemkind funcref; vector length is deliberately absent
        ],
    ];

    for payload in cases {
        let module = module_with_element_payload(payload);
        assert_eq!(
            parse_module(&module),
            Err(ParseError::UnsupportedElementSegmentMode(2)),
            "mode 2 must reject at the discriminant until its full parser/validator/runtime slice lands"
        );
    }
}
