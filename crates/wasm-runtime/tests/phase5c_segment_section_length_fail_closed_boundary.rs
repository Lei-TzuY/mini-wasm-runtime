use wasm_parser::{parse_module, ParseError};

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

#[test]
fn empty_segment_sections_reject_trailing_payload_bytes() {
    assert_eq!(
        parse_module(&module_with_section(9, &[0x00, 0xff])),
        Err(ParseError::SectionLengthMismatch(9))
    );
    assert_eq!(
        parse_module(&module_with_section(11, &[0x00, 0xff])),
        Err(ParseError::SectionLengthMismatch(11))
    );
}

#[test]
fn parsed_active_segments_reject_trailing_payload_bytes() {
    let element_payload = [
        0x01, // one segment
        0x00, // active mode 0
        0x41, 0x00, 0x0b, // i32.const 0; end
        0x00, // zero function indices
        0xff, // trailing garbage
    ];
    assert_eq!(
        parse_module(&module_with_section(9, &element_payload)),
        Err(ParseError::SectionLengthMismatch(9))
    );

    let data_payload = [
        0x01, // one segment
        0x00, // active mode 0
        0x41, 0x00, 0x0b, // i32.const 0; end
        0x00, // zero data bytes
        0xff, // trailing garbage
    ];
    assert_eq!(
        parse_module(&module_with_section(11, &data_payload)),
        Err(ParseError::SectionLengthMismatch(11))
    );
}
