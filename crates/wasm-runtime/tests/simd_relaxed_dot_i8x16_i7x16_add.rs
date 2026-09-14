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
fn run(a: [u8; 16], b: [u8; 16], c: [u8; 16]) -> [u8; 16] {
    let mut code = Vec::new();
    v128(&mut code, a);
    v128(&mut code, b);
    v128(&mut code, c);
    simd(&mut code, 275);
    let parsed = parse_module(&module(&code)).unwrap();
    validate(&parsed).unwrap();
    let mut inst = Instance::new(parsed).unwrap();
    match inst.invoke_export("run", &[]).unwrap().as_slice() {
        [Value::V128(v)] => **v,
        x => panic!("{x:?}"),
    }
}
fn i32x4(values: [i32; 4]) -> [u8; 16] {
    let mut out = [0u8; 16];
    for (lane, value) in values.into_iter().enumerate() {
        out[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    out
}
#[test]
fn relaxed_dot_add_executes_defined_signed_products_and_accumulator() {
    let mut a = [0u8; 16];
    let mut b = [0u8; 16];
    a[..4].copy_from_slice(&[2, (-3i8) as u8, 4, 5]);
    b[..4].copy_from_slice(&[4, 5, 6, 7]);
    let r = run(a, b, i32x4([10, 0, 0, 0]));
    assert_eq!(i32::from_le_bytes(r[..4].try_into().unwrap()), 62);
}
#[test]
fn relaxed_dot_add_uses_signed_rhs_and_saturating_pair_profile() {
    let a = [i8::MIN as u8; 16];
    let b = [i8::MIN as u8; 16];
    let r = run(a, b, i32x4([1, 2, 3, 4]));
    for lane in 0..4 {
        let start = lane * 4;
        let got = i32::from_le_bytes(r[start..start + 4].try_into().unwrap());
        assert_eq!(got, 2 * i32::from(i16::MAX) + (lane as i32 + 1));
    }
}
#[test]
fn relaxed_dot_add_requires_three_v128_operands() {
    let mut code = Vec::new();
    v128(&mut code, [0; 16]);
    v128(&mut code, [0; 16]);
    simd(&mut code, 275);
    let parsed = parse_module(&module(&code)).unwrap();
    assert!(matches!(
        validate(&parsed),
        Err(ValidationError::OperandStackUnderflow { .. })
    ));
}
#[test]
fn next_relaxed_simd_opcode_remains_fail_closed() {
    let mut code = Vec::new();
    v128(&mut code, [0; 16]);
    v128(&mut code, [0; 16]);
    v128(&mut code, [0; 16]);
    simd(&mut code, 276);
    let parsed = parse_module(&module(&code)).unwrap();
    assert!(validate(&parsed).is_err());
}
