use wasm_parser::{parse_module, ValueType};
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::ValidationError;

const I32: u8 = 0x7f;
const I64: u8 = 0x7e;

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

fn module(param: u8, result: u8, instructions: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x01, param, 0x01, result]);
    push_section(&mut bytes, 3, &[0x01, 0x00]);
    push_section(&mut bytes, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);

    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend(body);
    push_section(&mut bytes, 10, &code);
    bytes
}

fn validation_error(param: u8, result: u8, instructions: &[u8]) -> ValidationError {
    let bytes = module(param, result, instructions);
    match Instance::new(parse_module(&bytes).expect("fixture must parse"))
        .expect_err("fixture must fail validation")
    {
        RuntimeError::Validation(error) => error,
        other => panic!("expected validator rejection, got {other:?}"),
    }
}

fn assert_unreachable_stack_polymorphism(
    opcode: u8,
    param: u8,
    result: u8,
    final_value: &[u8],
    argument: Value,
    expected: Value,
) {
    let mut instructions = vec![
        0x02, 0x40, // block
        0x0c, 0x00, // br 0: the conversion below is unreachable
        opcode, 0x0b, // end block
    ];
    instructions.extend_from_slice(final_value);

    let bytes = module(param, result, &instructions);
    let mut instance = Instance::new(parse_module(&bytes).expect("fixture must parse"))
        .expect("unreachable conversion must validate");
    assert_eq!(
        instance
            .invoke_export("run", &[argument])
            .expect("execution must not trap"),
        Some(expected)
    );
}

#[test]
fn validator_rejects_extend_wrap_type_confusion() {
    let cases = [
        (0xa7, I32, I32, ValueType::I64, ValueType::I32),
        (0xac, I64, I64, ValueType::I32, ValueType::I64),
        (0xad, I64, I64, ValueType::I32, ValueType::I64),
    ];

    for (opcode, param, result, expected, actual) in cases {
        assert!(matches!(
            validation_error(param, result, &[0x20, 0x00, opcode]),
            ValidationError::TypeMismatch {
                expected: error_expected,
                actual: error_actual,
                ..
            } if error_expected == expected && error_actual == actual
        ));
    }
}

#[test]
fn validator_rejects_reachable_extend_wrap_underflow() {
    for (opcode, param, result) in [(0xa7, I64, I32), (0xac, I32, I64), (0xad, I32, I64)] {
        assert!(matches!(
            validation_error(param, result, &[opcode]),
            ValidationError::OperandStackUnderflow { .. }
        ));
    }
}

#[test]
fn extend_wrap_obey_unreachable_stack_polymorphism() {
    assert_unreachable_stack_polymorphism(
        0xa7,
        I64,
        I32,
        &[0x41, 0x00],
        Value::I64(1),
        Value::I32(0),
    );
    assert_unreachable_stack_polymorphism(
        0xac,
        I32,
        I64,
        &[0x42, 0x00],
        Value::I32(1),
        Value::I64(0),
    );
    assert_unreachable_stack_polymorphism(
        0xad,
        I32,
        I64,
        &[0x42, 0x00],
        Value::I32(1),
        Value::I64(0),
    );
}
