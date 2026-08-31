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

fn result_module(instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x01, 0x7f]);
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

fn assert_unreachable_rejected(instructions: &[u8], expected_offset: usize, expectation: &str) {
    let module = parse_module(&result_module(instructions))
        .expect("result-context unreachable fixture must parse");
    let error = match Instance::new(module).expect_err(expectation) {
        RuntimeError::Validation(error) => error,
        other => panic!("expected validator rejection, got {other:?}"),
    };
    assert!(matches!(
        error,
        ValidationError::UnsupportedOpcode {
            function: 0,
            offset,
            opcode: 0x00,
        } if offset == expected_offset
    ));
}

#[test]
fn function_result_requirement_does_not_partially_admit_unreachable() {
    assert_unreachable_rejected(
        &[0x00],
        0,
        "a result-producing function must reject unsupported unreachable before using stack polymorphism to satisfy its result",
    );
}

#[test]
fn block_result_requirement_does_not_partially_admit_unreachable() {
    assert_unreachable_rejected(
        &[
            0x02, 0x7f, // block (result i32)
            0x00, // unreachable would make the block result stack-polymorphic once admitted
            0x0b,
        ],
        2,
        "a result-producing block must not use unsupported unreachable to satisfy its result contract",
    );
}

#[test]
fn loop_result_requirement_does_not_partially_admit_unreachable() {
    assert_unreachable_rejected(
        &[
            0x03, 0x7f, // loop (result i32)
            0x00, // unreachable would make the loop body stack-polymorphic once admitted
            0x0b,
        ],
        2,
        "a result-producing loop must not use unsupported unreachable to satisfy its end-result contract",
    );
}

#[test]
fn if_result_requirement_does_not_partially_admit_unreachable() {
    assert_unreachable_rejected(
        &[
            0x41, 0x01, // true condition
            0x04, 0x7f, // if (result i32)
            0x00, // then arm would become stack-polymorphic once unreachable is admitted
            0x05, // else
            0x41, 0x2a, // i32.const 42 satisfies the else result
            0x0b,
        ],
        4,
        "a result-producing if must not use unsupported unreachable to satisfy its then-arm result contract",
    );
}
