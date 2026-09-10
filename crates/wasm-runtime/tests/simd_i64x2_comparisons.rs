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

fn push_i64_load(i: &mut Vec<u8>, offset: u32) {
    i.push(0x29);
    i.push(3);
    push_u32(i, offset);
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
        other => panic!("unexpected i64x2 comparison result: {other:?}"),
    }
}

fn comparison_result(lhs: [i64; 2], rhs: [i64; 2], subopcode: u32, lane: u32) -> i64 {
    let mut instructions = Vec::new();
    instructions.extend_from_slice(&[0x02, 0x40]);
    push_i64x2_const(&mut instructions, lhs);
    push_i64x2_const(&mut instructions, rhs);
    push_simd(&mut instructions, subopcode);
    instructions.push(0x1a);
    instructions.push(0x0b);
    push_i32_const(&mut instructions, 0);
    push_i64x2_const(&mut instructions, lhs);
    push_i64x2_const(&mut instructions, rhs);
    push_simd(&mut instructions, subopcode);
    push_v128_store(&mut instructions);
    push_i32_const(&mut instructions, 0);
    push_i64_load(&mut instructions, lane * 8);
    run_i64(&instructions)
}

#[test]
fn i64x2_comparisons_produce_canonical_masks_with_signed_ordering() {
    assert_eq!(comparison_result([7, -4], [7, 3], 214, 0), -1);
    assert_eq!(comparison_result([7, -4], [7, 3], 215, 1), -1);
    assert_eq!(comparison_result([-9, 8], [2, 8], 216, 0), -1);
    assert_eq!(comparison_result([-9, 8], [2, 3], 217, 1), -1);
    assert_eq!(comparison_result([5, 9], [5, 7], 218, 0), -1);
    assert_eq!(comparison_result([5, -1], [5, -1], 219, 1), -1);
    assert_eq!(comparison_result([1, 9], [2, 7], 214, 0), 0);
    assert_eq!(comparison_result([1, 9], [2, 7], 216, 1), 0);
}

#[test]
fn validator_rejects_i64x2_comparison_type_confusion() {
    let mut instructions = Vec::new();
    push_i64x2_const(&mut instructions, [1, 2]);
    push_i64_const(&mut instructions, 3);
    push_simd(&mut instructions, 214);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}

#[test]
fn adjacent_f32x4_min_frontier_remains_fail_closed() {
    let mut instructions = Vec::new();
    push_i64x2_const(&mut instructions, [1, 2]);
    push_i64x2_const(&mut instructions, [1, 3]);
    push_simd(&mut instructions, 232);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 232,
                ..
            }
        ))
    ));
}
