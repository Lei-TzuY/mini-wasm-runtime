use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError};
use wasm_validator::ValidationError;

const UNTERMINATED_U32: [u8; 5] = [0x80, 0x80, 0x80, 0x80, 0x80];
const OVERFLOWING_U32: [u8; 5] = [0x80, 0x80, 0x80, 0x80, 0x10];

const LOAD_OPCODES: [u8; 5] = [0x28, 0x2c, 0x2d, 0x2e, 0x2f];
const STORE_OPCODES: [u8; 3] = [0x36, 0x3a, 0x3b];

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

fn module_with_body(instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x00]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 5, &[0x01, 0x00, 0x01]);

    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn assert_malformed(instructions: &[u8], context: &str) {
    let module = parse_module(&module_with_body(instructions)).expect("fixture must parse");
    assert!(matches!(
        Instance::new(module).expect_err(context),
        RuntimeError::Validation(ValidationError::MalformedImmediate { function: 0, .. })
    ));
}

fn load_prefix(opcode: u8) -> Vec<u8> {
    vec![0x41, 0x00, opcode]
}

fn store_prefix(opcode: u8) -> Vec<u8> {
    vec![
        0x41, 0x00, // address
        0x41, 0x00, // value
        opcode,
    ]
}

fn assert_alignment_fail_closed(mut instructions: Vec<u8>, malformed: &[u8], context: &str) {
    instructions.extend_from_slice(malformed);
    assert_malformed(&instructions, context);
}

fn assert_offset_fail_closed(mut instructions: Vec<u8>, malformed: &[u8], context: &str) {
    instructions.push(0x00); // valid alignment exponent 0
    instructions.extend_from_slice(malformed);
    assert_malformed(&instructions, context);
}

#[test]
fn admitted_i32_load_memargs_fail_closed_on_malformed_leb() {
    for opcode in LOAD_OPCODES {
        assert_alignment_fail_closed(
            load_prefix(opcode),
            &UNTERMINATED_U32,
            "unterminated load alignment must fail closed",
        );
        assert_alignment_fail_closed(
            load_prefix(opcode),
            &OVERFLOWING_U32,
            "overflowing load alignment must fail closed",
        );
        assert_offset_fail_closed(
            load_prefix(opcode),
            &UNTERMINATED_U32,
            "unterminated load offset must fail closed",
        );
        assert_offset_fail_closed(
            load_prefix(opcode),
            &OVERFLOWING_U32,
            "overflowing load offset must fail closed",
        );
    }
}

#[test]
fn admitted_i32_store_memargs_fail_closed_on_malformed_leb() {
    for opcode in STORE_OPCODES {
        assert_alignment_fail_closed(
            store_prefix(opcode),
            &UNTERMINATED_U32,
            "unterminated store alignment must fail closed",
        );
        assert_alignment_fail_closed(
            store_prefix(opcode),
            &OVERFLOWING_U32,
            "overflowing store alignment must fail closed",
        );
        assert_offset_fail_closed(
            store_prefix(opcode),
            &UNTERMINATED_U32,
            "unterminated store offset must fail closed",
        );
        assert_offset_fail_closed(
            store_prefix(opcode),
            &OVERFLOWING_U32,
            "overflowing store offset must fail closed",
        );
    }
}
