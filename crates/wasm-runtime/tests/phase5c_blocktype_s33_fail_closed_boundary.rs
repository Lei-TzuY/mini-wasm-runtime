use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError};
use wasm_validator::ValidationError;

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

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn module_with_body(body: &[u8], append_end: bool) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x00]);
    push_section(&mut module, 3, &[0x01, 0x00]);

    let mut function_body = vec![0x00];
    function_body.extend_from_slice(body);
    if append_end {
        function_body.push(0x0b);
    }
    let mut code = vec![0x01];
    push_u32(&mut code, function_body.len() as u32);
    code.extend_from_slice(&function_body);
    push_section(&mut module, 10, &code);
    module
}

fn assert_malformed_blocktype(opener: u8, immediate: &[u8]) {
    let mut body = Vec::new();
    if opener == 0x04 {
        body.extend([0x41, 0x00]); // i32.const 0 condition
    }
    let expected_offset = body.len();
    body.push(opener);
    body.extend_from_slice(immediate);

    let module = parse_module(&module_with_body(&body, true))
        .expect("blocktype framing fixture must remain structurally parseable");
    let error = Instance::new(module).expect_err("malformed signed-33 blocktype must fail closed");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::MalformedImmediate {
            function: 0,
            offset,
        }) if offset == expected_offset
    ));
}

fn assert_all_structured_openers_fail(immediate: &[u8]) {
    for opener in [0x02, 0x03, 0x04] {
        assert_malformed_blocktype(opener, immediate);
    }
}

#[test]
fn truncated_bodies_fail_before_signed_33_blocktype_decoding() {
    for opener in [0x02, 0x03, 0x04] {
        let mut body = Vec::new();
        if opener == 0x04 {
            body.extend([0x41, 0x00]); // i32.const 0 condition
        }
        body.extend([opener, 0x80]); // continuation byte followed by body EOF

        let module = parse_module(&module_with_body(&body, false))
            .expect("truncated function body fixture must remain structurally parseable");
        assert!(matches!(
            Instance::new(module),
            Err(RuntimeError::Validation(ValidationError::MissingFunctionEnd {
                function: 0,
            }))
        ));
    }
}

#[test]
fn unterminated_signed_33_blocktypes_fail_closed() {
    assert_all_structured_openers_fail(&[0x80, 0x80, 0x80, 0x80, 0x80]);
}

#[test]
fn overflowing_signed_33_blocktypes_fail_closed() {
    assert_all_structured_openers_fail(&[0x80, 0x80, 0x80, 0x80, 0x20]);
}
