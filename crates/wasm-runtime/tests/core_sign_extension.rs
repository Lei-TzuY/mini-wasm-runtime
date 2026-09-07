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

fn invoke_i32(opcode: u8, value: i32) -> i32 {
    let bytes = module(I32, I32, &[0x20, 0x00, opcode]);
    let mut instance = Instance::new(parse_module(&bytes).unwrap()).unwrap();
    instance
        .invoke_export("run", &[Value::I32(value)])
        .unwrap()
        .unwrap()
        .as_i32()
}

fn invoke_i64(opcode: u8, value: i64) -> i64 {
    let bytes = module(I64, I64, &[0x20, 0x00, opcode]);
    let mut instance = Instance::new(parse_module(&bytes).unwrap()).unwrap();
    instance
        .invoke_export("run", &[Value::I64(value)])
        .unwrap()
        .unwrap()
        .as_i64()
}

#[test]
fn i32_sign_extension_uses_only_the_selected_low_lane() {
    for (opcode, cases) in [
        (
            0xc0,
            &[(0x0000_007f, 127), (0x0000_0080, -128), (0x1234_56ff, -1)][..],
        ),
        (
            0xc1,
            &[
                (0x0000_7fff, 32_767),
                (0x0000_8000, -32_768),
                (0x1234_ffff, -1),
            ][..],
        ),
    ] {
        for &(input, expected) in cases {
            assert_eq!(invoke_i32(opcode, input), expected);
        }
    }
}

#[test]
fn i64_sign_extension_uses_only_the_selected_low_lane() {
    let cases: &[(u8, i64, i64)] = &[
        (0xc2, 0x7f, 127),
        (0xc2, 0x80, -128),
        (0xc2, 0x1234_5678_9abc_deff, -1),
        (0xc3, 0x7fff, 32_767),
        (0xc3, 0x8000, -32_768),
        (0xc3, 0x1234_5678_9abc_ffff, -1),
        (0xc4, 0x7fff_ffff, 2_147_483_647),
        (0xc4, 0x8000_0000, -2_147_483_648),
        (0xc4, 0x1234_5678_ffff_ffff, -1),
    ];
    for &(opcode, input, expected) in cases {
        assert_eq!(invoke_i64(opcode, input), expected);
    }
}

#[test]
fn sign_extension_executes_inside_structured_control() {
    let bytes = module(I32, I32, &[0x02, I32, 0x20, 0x00, 0xc0, 0x0b]);
    let mut instance = Instance::new(parse_module(&bytes).unwrap()).unwrap();
    let result = instance
        .invoke_export("run", &[Value::I32(0x180)])
        .unwrap()
        .unwrap();
    assert_eq!(result, Value::I32(-128));
}

#[test]
fn validator_rejects_i32_sign_extension_type_confusion() {
    let bytes = module(I64, I32, &[0x20, 0x00, 0xc0]);
    let error = Instance::new(parse_module(&bytes).unwrap()).unwrap_err();
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::TypeMismatch {
            expected: ValueType::I32,
            actual: ValueType::I64,
            ..
        })
    ));
}

#[test]
fn validator_rejects_i64_sign_extension_type_confusion() {
    let bytes = module(I32, I64, &[0x20, 0x00, 0xc4]);
    let error = Instance::new(parse_module(&bytes).unwrap()).unwrap_err();
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::TypeMismatch {
            expected: ValueType::I64,
            actual: ValueType::I32,
            ..
        })
    ));
}
