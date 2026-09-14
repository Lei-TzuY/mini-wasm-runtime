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
fn module(result: u8, instructions: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x00, 0x01, result]);
    push_section(&mut bytes, 3, &[0x01, 0x00]);
    push_section(&mut bytes, 5, &[0x01, 0x00, 0x01]);
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
fn v128_const(i: &mut Vec<u8>, bytes: [u8; 16]) {
    i.extend_from_slice(&[0xfd, 0x0c]);
    i.extend_from_slice(&bytes);
}
fn invoke_store(sub: u8, align: u8, lane: u8, vector: [u8; 16], load: &[u8], result: u8) -> Value {
    let mut i = vec![0x41, 0x00];
    v128_const(&mut i, vector);
    i.extend_from_slice(&[0xfd, sub, align, 0x00, lane, 0x41, 0x00]);
    i.extend_from_slice(load);
    let parsed = parse_module(&module(result, &i)).expect("lane-store fixture parses");
    let mut instance = Instance::new(parsed).expect("lane-store fixture validates");
    instance
        .invoke_export_values("run", &[])
        .expect("lane-store executes")
        .remove(0)
}
#[test]
fn simd_lane_store_widths_execute() {
    let bytes = [
        0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
        0x10,
    ];
    assert_eq!(
        invoke_store(0x58, 0, 7, bytes, &[0x2d, 0x00, 0x00], 0x7f),
        Value::I32(0x88)
    );
    assert_eq!(
        invoke_store(0x59, 1, 3, bytes, &[0x2f, 0x01, 0x00], 0x7f),
        Value::I32(0x8877)
    );
    assert_eq!(
        invoke_store(0x5a, 2, 2, bytes, &[0x28, 0x02, 0x00], 0x7f),
        Value::I32(0xccbbaa99u32 as i32)
    );
    assert_eq!(
        invoke_store(0x5b, 3, 1, bytes, &[0x29, 0x03, 0x00], 0x7e),
        Value::I64(0x10ffeeddccbbaa99u64 as i64)
    );
}
#[test]
fn validator_rejects_lane_store_out_of_bounds() {
    let mut i = vec![0x41, 0x00];
    v128_const(&mut i, [0; 16]);
    i.extend_from_slice(&[0xfd, 0x58, 0x00, 0x00, 16, 0x41, 0x00]);
    let parsed = parse_module(&module(0x7f, &i)).expect("invalid lane fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::MalformedImmediate { .. }
        ))
    ));
}
#[test]
fn validator_rejects_lane_store_type_confusion() {
    let i = [
        0x41, 0x00, 0x41, 0x00, 0xfd, 0x58, 0x00, 0x00, 0x00, 0x41, 0x00,
    ];
    let parsed = parse_module(&module(0x7f, &i)).expect("type-confusion fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}
