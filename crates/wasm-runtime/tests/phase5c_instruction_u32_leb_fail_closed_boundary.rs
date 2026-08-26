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

fn build_module(instructions: &[u8]) -> Vec<u8> {
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

fn validation_error(instructions: &[u8], expectation: &str) -> ValidationError {
    let module = parse_module(&build_module(instructions))
        .expect("fixture must remain structurally parseable");
    match Instance::new(module).expect_err(expectation) {
        RuntimeError::Validation(error) => error,
        other => panic!("expected validator rejection, got {other:?}"),
    }
}

fn assert_malformed(instructions: &[u8], expectation: &str) {
    assert!(matches!(
        validation_error(instructions, expectation),
        ValidationError::MalformedImmediate { function: 0, .. }
    ));
}

const UNTERMINATED_U32: [u8; 5] = [0x80, 0x80, 0x80, 0x80, 0x80];

fn instruction_with_prefix(prefix: &[u8]) -> Vec<u8> {
    let mut instructions = prefix.to_vec();
    instructions.extend_from_slice(&UNTERMINATED_U32);
    instructions
}

#[test]
fn branch_depth_immediates_fail_closed() {
    assert_malformed(
        &instruction_with_prefix(&[0x0c]),
        "br depth must reject malformed u32 LEB",
    );
    assert_malformed(
        &instruction_with_prefix(&[0x0d]),
        "br_if depth must reject malformed u32 LEB",
    );
}

#[test]
fn direct_and_indirect_call_immediates_fail_closed() {
    assert_malformed(
        &instruction_with_prefix(&[0x10]),
        "call target must reject malformed u32 LEB",
    );
    assert_malformed(
        &instruction_with_prefix(&[0x11]),
        "call_indirect type index must reject malformed u32 LEB",
    );
    assert_malformed(
        &instruction_with_prefix(&[0x11, 0x00]),
        "call_indirect table index must reject malformed u32 LEB",
    );
}

#[test]
fn local_and_global_indices_fail_closed() {
    assert_malformed(
        &instruction_with_prefix(&[0x20]),
        "local.get index must reject malformed u32 LEB",
    );
    assert_malformed(
        &instruction_with_prefix(&[0x21]),
        "local.set index must reject malformed u32 LEB",
    );
    assert_malformed(
        &instruction_with_prefix(&[0x22]),
        "local.tee index must reject malformed u32 LEB",
    );
    assert_malformed(
        &instruction_with_prefix(&[0x23]),
        "global.get index must reject malformed u32 LEB",
    );
    assert_malformed(
        &instruction_with_prefix(&[0x24]),
        "global.set index must reject malformed u32 LEB",
    );
}

#[test]
fn memory_indices_fail_closed() {
    assert_malformed(
        &instruction_with_prefix(&[0x3f]),
        "memory.size index must reject malformed u32 LEB",
    );
    assert_malformed(
        &instruction_with_prefix(&[0x40]),
        "memory.grow index must reject malformed u32 LEB",
    );
}
