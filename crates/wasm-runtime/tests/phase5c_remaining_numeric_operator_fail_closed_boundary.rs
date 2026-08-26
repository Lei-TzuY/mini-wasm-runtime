use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError};
use wasm_validator::ValidationError;

const I32: u8 = 0x7f;
const I64: u8 = 0x7e;
const F32: u8 = 0x7d;
const F64: u8 = 0x7c;

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

fn module_with_result(result_type: u8, instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x01, result_type]);
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

fn push_operand(bytes: &mut Vec<u8>, value_type: u8) {
    match value_type {
        I32 => bytes.extend_from_slice(&[0x41, 0x01]),
        I64 => bytes.extend_from_slice(&[0x42, 0x01]),
        F32 => {
            bytes.push(0x43);
            bytes.extend_from_slice(&1.0f32.to_bits().to_le_bytes());
        }
        F64 => {
            bytes.push(0x44);
            bytes.extend_from_slice(&1.0f64.to_bits().to_le_bytes());
        }
        other => panic!("unexpected numeric fixture type 0x{other:02x}"),
    }
}

fn validator_error(bytes: &[u8]) -> ValidationError {
    let module = parse_module(bytes).expect("numeric operator boundary fixture must parse");
    match Instance::new(module).expect_err("unsupported numeric operator must fail closed") {
        RuntimeError::Validation(error) => error,
        other => panic!("expected validator rejection, got {other:?}"),
    }
}

fn assert_opcode_range_fail_closed(
    result_type: u8,
    operand_type: u8,
    operand_count: usize,
    opcodes: impl IntoIterator<Item = u8>,
) {
    for opcode in opcodes {
        let mut instructions = Vec::new();
        for _ in 0..operand_count {
            push_operand(&mut instructions, operand_type);
        }
        instructions.push(opcode);
        assert!(matches!(
            validator_error(&module_with_result(result_type, &instructions)),
            ValidationError::UnsupportedOpcode {
                function: 0,
                opcode: actual,
                ..
            } if actual == opcode
        ));
    }
}

#[test]
fn all_fifty_remaining_mvp_numeric_operators_remain_fail_closed() {
    assert_opcode_range_fail_closed(I32, I32, 1, 0x67..=0x69);
    assert_opcode_range_fail_closed(I32, I32, 2, 0x6d..=0x78);
    assert_opcode_range_fail_closed(I64, I64, 1, 0x79..=0x7b);
    assert_opcode_range_fail_closed(I64, I64, 2, 0x7f..=0x8a);
    assert_opcode_range_fail_closed(F32, F32, 1, 0x8b..=0x91);
    assert_opcode_range_fail_closed(F32, F32, 2, 0x96..=0x98);
    assert_opcode_range_fail_closed(F64, F64, 1, 0x99..=0x9f);
    assert_opcode_range_fail_closed(F64, F64, 2, 0xa4..=0xa6);
}

#[test]
fn unreachable_polymorphism_does_not_admit_remaining_numeric_operator() {
    let module = module_with_result(
        I32,
        &[
            0x02, I32, // block (result i32)
            0x41, 0x00, // branch result
            0x0c, 0x00, // br 0
            0x41, 0x01, // unreachable i32 operand
            0x67, // i32.clz must still fail closed
            0x0b, // end block
        ],
    );

    assert!(matches!(
        validator_error(&module),
        ValidationError::UnsupportedOpcode {
            function: 0,
            opcode: 0x67,
            ..
        }
    ));
}
