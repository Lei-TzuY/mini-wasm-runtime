use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::ValidationError;

fn push_u32(bytes: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut b = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            b |= 0x80;
        }
        bytes.push(b);
        if value == 0 {
            break;
        }
    }
}
fn section(m: &mut Vec<u8>, id: u8, p: &[u8]) {
    m.push(id);
    push_u32(m, p.len() as u32);
    m.extend_from_slice(p);
}
fn module(result: u8, body: &[u8]) -> Vec<u8> {
    let mut m = vec![0, 0x61, 0x73, 0x6d, 1, 0, 0, 0];
    section(&mut m, 1, &[1, 0x60, 0, 1, result]);
    section(&mut m, 3, &[1, 0]);
    section(&mut m, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);
    let mut b = vec![0];
    b.extend_from_slice(body);
    b.push(0x0b);
    let mut c = vec![1];
    push_u32(&mut c, b.len() as u32);
    c.extend(b);
    section(&mut m, 10, &c);
    m
}
fn simd(v: &mut Vec<u8>, op: u8) {
    v.extend_from_slice(&[0xfd, op]);
}
fn invoke(result: u8, body: &[u8]) -> Value {
    let mut i = Instance::new(parse_module(&module(result, body)).unwrap()).unwrap();
    i.invoke_export_values("run", &[]).unwrap().remove(0)
}

#[test]
fn i32x4_replace_lane_executes() {
    let mut b = vec![0x41, 7];
    simd(&mut b, 17);
    b.extend_from_slice(&[0x41, 42]);
    simd(&mut b, 28);
    b.push(2);
    simd(&mut b, 27);
    b.push(2);
    assert!(matches!(invoke(0x7f, &b), Value::I32(42)));
}
#[test]
fn i64x2_splat_extract_replace_execute() {
    let mut b = vec![0x42, 7];
    simd(&mut b, 18);
    b.extend_from_slice(&[0x42, 0x2a]);
    simd(&mut b, 30);
    b.push(1);
    simd(&mut b, 29);
    b.push(1);
    assert!(matches!(invoke(0x7e, &b), Value::I64(42)));
}
#[test]
fn float_lane_ops_preserve_bits() {
    let f32_bits = 0x7fc0_1234u32;
    let mut b = vec![0x43];
    b.extend_from_slice(&f32_bits.to_le_bytes());
    simd(&mut b, 19);
    simd(&mut b, 31);
    b.push(3);
    match invoke(0x7d, &b) {
        Value::F32(v) => assert_eq!(v.to_bits(), f32_bits),
        x => panic!("{x:?}"),
    }
    let f64_bits = 0x7ff8_0000_0000_1234u64;
    let mut b = vec![0x44];
    b.extend_from_slice(&f64_bits.to_le_bytes());
    simd(&mut b, 20);
    simd(&mut b, 33);
    b.push(1);
    match invoke(0x7c, &b) {
        Value::F64(v) => assert_eq!(v.to_bits(), f64_bits),
        x => panic!("{x:?}"),
    }
}
#[test]
fn lane_ops_work_inside_structured_control() {
    let mut b = vec![0x02, 0x7e, 0x42, 9];
    simd(&mut b, 18);
    simd(&mut b, 29);
    b.push(1);
    b.push(0x0b);
    assert!(matches!(invoke(0x7e, &b), Value::I64(9)));
}
#[test]
fn validator_rejects_invalid_lane_and_scalar_type() {
    let mut bad_lane = vec![0x42, 0];
    simd(&mut bad_lane, 18);
    simd(&mut bad_lane, 29);
    bad_lane.push(2);
    let parsed = parse_module(&module(0x7e, &bad_lane)).unwrap();
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::MalformedImmediate { .. }
        ))
    ));
    let mut bad_type = vec![0x41, 0];
    simd(&mut bad_type, 18);
    let parsed = parse_module(&module(0x7b, &bad_type)).unwrap();
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}
