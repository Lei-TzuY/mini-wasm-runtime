use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::ValidationError;

fn u32leb(out: &mut Vec<u8>, mut v: u32) {
    loop {
        let mut b = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            b |= 0x80;
        }
        out.push(b);
        if v == 0 {
            break;
        }
    }
}
fn section(m: &mut Vec<u8>, id: u8, p: &[u8]) {
    m.push(id);
    u32leb(m, p.len() as u32);
    m.extend_from_slice(p);
}
fn module(ins: &[u8]) -> Vec<u8> {
    let mut m = vec![0, 97, 115, 109, 1, 0, 0, 0];
    section(&mut m, 1, &[1, 0x60, 0, 1, 0x7f]);
    section(&mut m, 3, &[1, 0]);
    section(&mut m, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);
    let mut b = vec![0];
    b.extend_from_slice(ins);
    b.push(0x0b);
    let mut c = vec![1];
    u32leb(&mut c, b.len() as u32);
    c.extend(b);
    section(&mut m, 10, &c);
    m
}
fn simd(i: &mut Vec<u8>, op: u32) {
    i.push(0xfd);
    u32leb(i, op);
}
fn splat(i: &mut Vec<u8>, x: i16) {
    simd(i, 12);
    for _ in 0..8 {
        i.extend_from_slice(&x.to_le_bytes());
    }
}
fn lane(lhs: i16, rhs: i16) -> i32 {
    let mut i = Vec::new();
    splat(&mut i, lhs);
    splat(&mut i, rhs);
    simd(&mut i, 273);
    simd(&mut i, 24);
    i.push(0);
    let p = parse_module(&module(&i)).unwrap();
    let mut x = Instance::new(p).unwrap();
    match x.invoke_export_values("run", &[]).unwrap().as_slice() {
        [Value::I32(v)] => *v,
        _ => panic!(),
    }
}
#[test]
fn relaxed_q15mulr_executes_rounded_q15_lanes() {
    assert_eq!(lane(16384, 16384), 8192);
    assert_eq!(lane(-16384, 16384), -8192);
}
#[test]
fn relaxed_q15mulr_chooses_saturating_overflow_result() {
    assert_eq!(lane(i16::MIN, i16::MIN), i16::MAX as i32);
}
#[test]
fn validator_rejects_relaxed_q15mulr_type_confusion() {
    let mut i = Vec::new();
    splat(&mut i, 1);
    i.push(0x41);
    i.push(1);
    simd(&mut i, 273);
    let p = parse_module(&module(&i)).unwrap();
    assert!(matches!(
        Instance::new(p),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}
#[test]
fn next_relaxed_simd_frontier_remains_fail_closed() {
    let mut i = Vec::new();
    splat(&mut i, 1);
    splat(&mut i, 1);
    simd(&mut i, 275);
    simd(&mut i, 24);
    i.push(0);
    let p = parse_module(&module(&i)).unwrap();
    assert!(matches!(
        Instance::new(p),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 275,
                ..
            }
        ))
    ));
}
