use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};
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

fn module(instructions: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x00, 0x01, 0x7f]);
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

fn run_i32(instructions: &[u8]) -> i32 {
    let parsed = parse_module(&module(instructions)).expect("i8x16 shift fixture must parse");
    let mut instance = Instance::new(parsed).expect("i8x16 shift fixture must validate");
    match instance
        .invoke_export_values("run", &[])
        .expect("i8x16 shift fixture must execute")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected i8x16 shift result: {other:?}"),
    }
}

#[test]
fn i8x16_shift_family_masks_counts_and_preserves_signedness() {
    assert_eq!(
        run_i32(&[0x41, 0x40, 0xfd, 0x0f, 0x41, 0x09, 0xfd, 0x6b, 0xfd, 0x15, 0x00]),
        -128
    );
    assert_eq!(
        run_i32(&[0x41, 0x7e, 0xfd, 0x0f, 0x41, 0x01, 0xfd, 0x6c, 0xfd, 0x15, 0x00]),
        -1
    );
    assert_eq!(
        run_i32(&[0x41, 0x80, 0x7f, 0xfd, 0x0f, 0x41, 0x01, 0xfd, 0x6d, 0xfd, 0x16, 0x00]),
        64
    );
}

#[test]
fn i8x16_shifts_execute_inside_structured_control() {
    assert_eq!(
        run_i32(&[
            0x02, 0x7f, 0x41, 0x01, 0xfd, 0x0f, 0x41, 0x03, 0xfd, 0x6b, 0xfd, 0x16, 0x00, 0x0b
        ]),
        8
    );
}

#[test]
fn validator_rejects_i8x16_shift_type_confusion() {
    let bytes = module(&[0x41, 0x00, 0x41, 0x01, 0xfd, 0x6b, 0xfd, 0x16, 0x00]);
    let parsed = parse_module(&bytes).expect("type-confusion fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}

#[test]
fn adjacent_i8x16_arithmetic_remains_fail_closed() {
    let bytes = module(&[
        0x41, 0x01, 0xfd, 0x0f, 0x41, 0x02, 0xfd, 0x0f, 0xfd, 0x6e, 0xfd, 0x16, 0x00,
    ]);
    let parsed = parse_module(&bytes).expect("adjacent arithmetic fixture must parse");
    assert!(Instance::new(parsed).is_err());
}
