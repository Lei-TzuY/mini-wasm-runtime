use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};
use wasm_validator::{validate, ValidationError};

fn u32leb(mut n: u32, out: &mut Vec<u8>) {
    loop {
        let mut b = (n & 0x7f) as u8;
        n >>= 7;
        if n != 0 {
            b |= 0x80;
        }
        out.push(b);
        if n == 0 {
            break;
        }
    }
}
fn simd(out: &mut Vec<u8>, op: u32) {
    out.push(0xfd);
    u32leb(op, out);
}
fn v128(out: &mut Vec<u8>, bytes: [u8; 16]) {
    simd(out, 12);
    out.extend_from_slice(&bytes);
}
fn module(code: &[u8]) -> Vec<u8> {
    let mut body = vec![0x00];
    body.extend_from_slice(code);
    body.push(0x0b);
    let type_sec = vec![0x01, 0x60, 0x00, 0x01, 0x7b];
    let func_sec = vec![0x01, 0x00];
    let export = vec![0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00];
    let mut code_sec = vec![0x01];
    u32leb(body.len() as u32, &mut code_sec);
    code_sec.extend(body);
    let mut m = b"\0asm\x01\0\0\0".to_vec();
    for (id, sec) in [(1, type_sec), (3, func_sec), (7, export), (10, code_sec)] {
        m.push(id);
        u32leb(sec.len() as u32, &mut m);
        m.extend(sec);
    }
    m
}
fn run(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let mut c = Vec::new();
    v128(&mut c, a);
    v128(&mut c, b);
    simd(&mut c, 274);
    let parsed = parse_module(&module(&c)).unwrap();
    validate(&parsed).unwrap();
    let mut inst = Instance::new(parsed).unwrap();
    match inst.invoke_export("run", &[]).unwrap().as_slice() {
        [Value::V128(v)] => **v,
        x => panic!("{x:?}"),
    }
}
#[test]
fn relaxed_dot_executes_signed_pairwise_products() {
    let mut a = [0u8; 16];
    let mut b = [0u8; 16];
    a[0] = 2;
    a[1] = (-3i8) as u8;
    b[0] = 4;
    b[1] = 5;
    let r = run(a, b);
    assert_eq!(i16::from_le_bytes([r[0], r[1]]), -7);
}
#[test]
fn relaxed_dot_chooses_signed_rhs_and_saturating_profile() {
    let a = [i8::MIN as u8; 16];
    let b = [i8::MIN as u8; 16];
    let r = run(a, b);
    for lane in 0..8 {
        let i = lane * 2;
        assert_eq!(i16::from_le_bytes([r[i], r[i + 1]]), i16::MAX);
    }
}
#[test]
fn relaxed_dot_requires_two_v128_operands() {
    let mut c = vec![0x41, 0x01];
    simd(&mut c, 274);
    let parsed = parse_module(&module(&c)).unwrap();
    assert!(matches!(
        validate(&parsed),
        Err(ValidationError::TypeMismatch { .. })
    ));
}
#[test]
fn next_relaxed_simd_opcode_remains_fail_closed() {
    let mut c = Vec::new();
    v128(&mut c, [0; 16]);
    v128(&mut c, [0; 16]);
    simd(&mut c, 275);
    let parsed = parse_module(&module(&c)).unwrap();
    assert!(validate(&parsed).is_err());
}
