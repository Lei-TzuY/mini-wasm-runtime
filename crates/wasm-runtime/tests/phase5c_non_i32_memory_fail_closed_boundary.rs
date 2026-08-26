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

fn memory_module(result_type: Option<u8>, instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let mut type_section = vec![0x01, 0x60, 0x00];
    match result_type {
        Some(result_type) => type_section.extend_from_slice(&[0x01, result_type]),
        None => type_section.push(0x00),
    }
    push_section(&mut module, 1, &type_section);
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

fn validator_error(bytes: &[u8]) -> ValidationError {
    let module = parse_module(bytes).expect("non-i32 memory boundary fixture must parse");
    match Instance::new(module).expect_err("non-i32 memory opcode must fail closed") {
        RuntimeError::Validation(error) => error,
        other => panic!("expected validator rejection, got {other:?}"),
    }
}

fn assert_unsupported(result_type: Option<u8>, instructions: &[u8], opcode: u8) {
    assert!(matches!(
        validator_error(&memory_module(result_type, instructions)),
        ValidationError::UnsupportedOpcode {
            function: 0,
            opcode: actual,
            ..
        } if actual == opcode
    ));
}

#[test]
fn all_non_i32_mvp_loads_remain_fail_closed() {
    let cases = [
        (0x29, 0x7e, 3),
        (0x2a, 0x7d, 2),
        (0x2b, 0x7c, 3),
        (0x30, 0x7e, 0),
        (0x31, 0x7e, 0),
        (0x32, 0x7e, 1),
        (0x33, 0x7e, 1),
        (0x34, 0x7e, 2),
        (0x35, 0x7e, 2),
    ];

    for (opcode, result_type, alignment) in cases {
        let instructions = [0x41, 0x00, opcode, alignment, 0x00];
        assert_unsupported(Some(result_type), &instructions, opcode);
    }
}

#[test]
fn all_non_i32_mvp_stores_remain_fail_closed() {
    let cases: [(u8, &[u8], u8); 6] = [
        (0x37, &[0x42, 0x00], 3),
        (0x38, &[0x43, 0x00, 0x00, 0x00, 0x00], 2),
        (
            0x39,
            &[0x44, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            3,
        ),
        (0x3c, &[0x42, 0x00], 0),
        (0x3d, &[0x42, 0x00], 1),
        (0x3e, &[0x42, 0x00], 2),
    ];

    for (opcode, value, alignment) in cases {
        let mut instructions = vec![0x41, 0x00];
        instructions.extend_from_slice(value);
        instructions.extend_from_slice(&[opcode, alignment, 0x00]);
        assert_unsupported(None, &instructions, opcode);
    }
}

#[test]
fn unreachable_polymorphism_does_not_admit_non_i32_memory_opcode() {
    let module = memory_module(
        Some(0x7e),
        &[
            0x02, 0x7e, // block (result i64)
            0x42, 0x00, // branch result
            0x0c, 0x00, // br 0
            0x41, 0x00, // unreachable address
            0x29, 0x03, 0x00, // i64.load align=8 offset=0 must still fail closed
            0x0b, // end block
        ],
    );

    assert!(matches!(
        validator_error(&module),
        ValidationError::UnsupportedOpcode {
            function: 0,
            opcode: 0x29,
            ..
        }
    ));
}
