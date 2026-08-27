use wasm_parser::{parse_module, ValueType};
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::ValidationError;

const I32: u8 = 0x7f;
const I64: u8 = 0x7e;
const F32: u8 = 0x7d;
const F64: u8 = 0x7c;

const I32_EQZ: [u8; 1] = [0x45];
const I32_COMPARE: [u8; 10] = [0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e, 0x4f];
const I64_EQZ: [u8; 1] = [0x50];
const I64_COMPARE: [u8; 10] = [0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a];
const F32_COMPARE: [u8; 6] = [0x5b, 0x5c, 0x5d, 0x5e, 0x5f, 0x60];
const F64_COMPARE: [u8; 6] = [0x61, 0x62, 0x63, 0x64, 0x65, 0x66];

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

fn module(param: u8, instructions: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x01, param, 0x01, I32]);
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

fn validation_error(param: u8, instructions: &[u8]) -> ValidationError {
    let bytes = module(param, instructions);
    match Instance::new(parse_module(&bytes).expect("fixture must parse"))
        .expect_err("fixture must fail validation")
    {
        RuntimeError::Validation(error) => error,
        other => panic!("expected validator rejection, got {other:?}"),
    }
}

fn assert_type_confusion(
    opcode: u8,
    arity: usize,
    param: u8,
    expected: ValueType,
    actual: ValueType,
) {
    let mut instructions = Vec::new();
    for _ in 0..arity {
        instructions.extend([0x20, 0x00]);
    }
    instructions.push(opcode);

    assert!(matches!(
        validation_error(param, &instructions),
        ValidationError::TypeMismatch {
            expected: error_expected,
            actual: error_actual,
            ..
        } if error_expected == expected && error_actual == actual
    ));
}

fn assert_reachable_underflow(opcode: u8) {
    assert!(matches!(
        validation_error(I32, &[opcode]),
        ValidationError::OperandStackUnderflow { .. }
    ));
}

fn assert_unreachable_stack_polymorphism(opcode: u8) {
    let instructions = [
        0x02, 0x40, // block
        0x0c, 0x00, // br 0: the comparison below is unreachable
        opcode, 0x0b, // end block
        0x41, 0x00, // final i32 result
    ];
    let bytes = module(I32, &instructions);
    let mut instance = Instance::new(parse_module(&bytes).expect("fixture must parse"))
        .expect("unreachable comparison must validate polymorphically");
    assert_eq!(
        instance
            .invoke_export("run", &[Value::I32(1)])
            .expect("execution must not trap"),
        Some(Value::I32(0))
    );
}

#[test]
fn validator_rejects_comparison_type_confusion_for_every_admitted_opcode() {
    for opcode in I32_EQZ {
        assert_type_confusion(opcode, 1, I64, ValueType::I32, ValueType::I64);
    }
    for opcode in I32_COMPARE {
        assert_type_confusion(opcode, 2, I64, ValueType::I32, ValueType::I64);
    }
    for opcode in I64_EQZ {
        assert_type_confusion(opcode, 1, I32, ValueType::I64, ValueType::I32);
    }
    for opcode in I64_COMPARE {
        assert_type_confusion(opcode, 2, I32, ValueType::I64, ValueType::I32);
    }
    for opcode in F32_COMPARE {
        assert_type_confusion(opcode, 2, F64, ValueType::F32, ValueType::F64);
    }
    for opcode in F64_COMPARE {
        assert_type_confusion(opcode, 2, F32, ValueType::F64, ValueType::F32);
    }
}

#[test]
fn validator_rejects_reachable_comparison_underflow_for_every_admitted_opcode() {
    for opcode in I32_EQZ {
        assert_reachable_underflow(opcode);
    }
    for opcode in I32_COMPARE {
        assert_reachable_underflow(opcode);
    }
    for opcode in I64_EQZ {
        assert_reachable_underflow(opcode);
    }
    for opcode in I64_COMPARE {
        assert_reachable_underflow(opcode);
    }
    for opcode in F32_COMPARE {
        assert_reachable_underflow(opcode);
    }
    for opcode in F64_COMPARE {
        assert_reachable_underflow(opcode);
    }
}

#[test]
fn comparisons_obey_unreachable_stack_polymorphism_for_every_admitted_opcode() {
    for opcode in I32_EQZ {
        assert_unreachable_stack_polymorphism(opcode);
    }
    for opcode in I32_COMPARE {
        assert_unreachable_stack_polymorphism(opcode);
    }
    for opcode in I64_EQZ {
        assert_unreachable_stack_polymorphism(opcode);
    }
    for opcode in I64_COMPARE {
        assert_unreachable_stack_polymorphism(opcode);
    }
    for opcode in F32_COMPARE {
        assert_unreachable_stack_polymorphism(opcode);
    }
    for opcode in F64_COMPARE {
        assert_unreachable_stack_polymorphism(opcode);
    }
}
