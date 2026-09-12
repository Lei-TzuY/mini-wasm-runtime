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
fn push_i64_const(bytes: &mut Vec<u8>, mut value: i64) {
    bytes.push(0x42);
    loop {
        let byte = (value as u8) & 0x7f;
        value >>= 7;
        let sign = byte & 0x40 != 0;
        let done = (value == 0 && !sign) || (value == -1 && sign);
        bytes.push(if done { byte } else { byte | 0x80 });
        if done {
            break;
        }
    }
}
fn push_i32_const(bytes: &mut Vec<u8>, mut value: i32) {
    bytes.push(0x41);
    loop {
        let byte = (value as u8) & 0x7f;
        value >>= 7;
        let sign = byte & 0x40 != 0;
        let done = (value == 0 && !sign) || (value == -1 && sign);
        bytes.push(if done { byte } else { byte | 0x80 });
        if done {
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
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0, 0, 0];
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x00, 0x01, 0x7e]);
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
fn push_simd(i: &mut Vec<u8>, sub: u32) {
    i.push(0xfd);
    push_u32(i, sub);
}
fn push_i64x2_const(i: &mut Vec<u8>, lanes: [i64; 2]) {
    push_simd(i, 12);
    for lane in lanes {
        i.extend_from_slice(&lane.to_le_bytes());
    }
}
fn push_v128_store(i: &mut Vec<u8>) {
    push_simd(i, 11);
    i.extend_from_slice(&[4, 0]);
}
fn push_i64_load(i: &mut Vec<u8>) {
    i.extend_from_slice(&[0x29, 3, 0]);
}
fn run_i64(instructions: &[u8]) -> i64 {
    let parsed = parse_module(&module(instructions)).expect("fixture parses");
    let mut instance = Instance::new(parsed).expect("fixture validates");
    match instance
        .invoke_export_values("run", &[])
        .expect("fixture executes")
        .as_slice()
    {
        [Value::I64(value)] => *value,
        other => panic!("unexpected i64x2 shift result: {other:?}"),
    }
}
#[test]
fn i64x2_shift_family_masks_counts_and_preserves_signedness() {
    let mut shl = Vec::new();
    push_i32_const(&mut shl, 0);
    push_i64x2_const(&mut shl, [0x4000_0000_0000_0000, 0]);
    push_i32_const(&mut shl, 65);
    push_simd(&mut shl, 203);
    push_v128_store(&mut shl);
    push_i32_const(&mut shl, 0);
    push_i64_load(&mut shl);
    assert_eq!(run_i64(&shl), i64::MIN);
    let mut shr_s = Vec::new();
    push_i32_const(&mut shr_s, 0);
    push_i64x2_const(&mut shr_s, [-2, 0]);
    push_i32_const(&mut shr_s, 1);
    push_simd(&mut shr_s, 204);
    push_v128_store(&mut shr_s);
    push_i32_const(&mut shr_s, 0);
    push_i64_load(&mut shr_s);
    assert_eq!(run_i64(&shr_s), -1);
    let mut shr_u = Vec::new();
    push_i32_const(&mut shr_u, 0);
    push_i64x2_const(&mut shr_u, [i64::MIN, 0]);
    push_i32_const(&mut shr_u, 1);
    push_simd(&mut shr_u, 205);
    push_v128_store(&mut shr_u);
    push_i32_const(&mut shr_u, 0);
    push_i64_load(&mut shr_u);
    assert_eq!(run_i64(&shr_u), 0x4000_0000_0000_0000);
}
#[test]
fn validator_rejects_i64_shift_count_type_confusion() {
    let mut instructions = Vec::new();
    push_i32_const(&mut instructions, 0);
    push_i64x2_const(&mut instructions, [1, 2]);
    push_i64_const(&mut instructions, 1);
    push_simd(&mut instructions, 203);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}
#[test]
fn adjacent_f32x4_f64x2_min_frontier_remains_fail_closed() {
    let mut instructions = Vec::new();
    push_i64x2_const(&mut instructions, [1, 2]);
    push_i64x2_const(&mut instructions, [3, 4]);
    push_simd(&mut instructions, 257);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 257,
                ..
            }
        ))
    ));
}
