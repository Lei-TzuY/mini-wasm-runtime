use wasm_parser::parse_module;
use wasm_validator::{validate, ValidationError};

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

fn module_with_instructions(instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x00]);
    push_section(&mut module, 3, &[0x01, 0x00]);

    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn assert_unsupported_before_immediate_decode(instructions: &[u8], opcode: u8, expectation: &str) {
    let module = parse_module(&module_with_instructions(instructions))
        .expect("reference-immediate boundary fixture must remain parseable");
    assert!(
        matches!(
            validate(&module),
            Err(ValidationError::UnsupportedOpcode {
                function: 0,
                opcode: actual,
                ..
            }) if actual == opcode
        ),
        "{expectation}"
    );
}

#[test]
fn ref_null_immediate_framing_cannot_bypass_the_unsupported_opcode_gate() {
    let cases: &[(&[u8], &str)] = &[
        (
            &[0xd0],
            "truncated ref.null must fail at the unsupported opcode boundary",
        ),
        (
            &[0xd0, 0x00],
            "an invalid ref.null heap-type byte must not leak partial reference decoding",
        ),
    ];

    for (instructions, expectation) in cases {
        assert_unsupported_before_immediate_decode(instructions, 0xd0, expectation);
    }
}

#[test]
fn ref_func_immediate_framing_cannot_bypass_the_unsupported_opcode_gate() {
    let cases: &[(&[u8], &str)] = &[
        (
            &[0xd2],
            "truncated ref.func must fail at the unsupported opcode boundary",
        ),
        (
            &[0xd2, 0x80],
            "an unterminated ref.func index LEB must not leak partial reference decoding",
        ),
    ];

    for (instructions, expectation) in cases {
        assert_unsupported_before_immediate_decode(instructions, 0xd2, expectation);
    }
}
