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

fn module_with_body(body: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(body);
    module.push(10);
    push_u32(&mut module, code.len() as u32);
    module.extend_from_slice(&code);
    module
}

#[test]
fn empty_instruction_stream_after_locals_requires_final_end() {
    assert_eq!(
        parse_module(&module_with_body(&[0x00])),
        Err(ParseError::FunctionBodyMissingEnd)
    );
}

#[test]
fn unterminated_instruction_stream_fails_closed() {
    assert_eq!(
        parse_module(&module_with_body(&[
            0x00, // zero local groups
            0x41, 0x00, // i32.const 0
        ])),
        Err(ParseError::FunctionBodyMissingEnd)
    );
}

#[test]
fn embedded_end_does_not_replace_the_final_body_terminator() {
    assert_eq!(
        parse_module(&module_with_body(&[
            0x00, // zero local groups
            0x0b, // embedded end byte
            0x01, // trailing instruction byte means the body itself is not end-terminated
        ])),
        Err(ParseError::FunctionBodyMissingEnd)
    );
}

#[test]
fn exact_final_end_is_accepted() {
    let module = parse_module(&module_with_body(&[
        0x00, // zero local groups
        0x0b, // final end
    ]))
    .expect("a function body ending exactly in end must parse");

    assert_eq!(module.code.len(), 1);
    assert!(module.code[0].locals.is_empty());
    assert_eq!(module.code[0].code, vec![0x0b]);
}
