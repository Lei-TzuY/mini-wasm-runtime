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

fn push_i32(bytes: &mut Vec<u8>, mut value: i32) {
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

fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn module(instructions: &[u8]) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 0, 1, 0x7f]);
    section(&mut module, 3, &[1, 0]);
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);

    let mut body = vec![0];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![1];
    push_u32(&mut code, body.len() as u32);
    code.extend(body);
    section(&mut module, 10, &code);
    module
}

fn simd(code: &mut Vec<u8>, subopcode: u32) {
    code.push(0xfd);
    push_u32(code, subopcode);
}

fn push_i32x4(code: &mut Vec<u8>, lanes: [i32; 4]) {
    simd(code, 12);
    for lane in lanes {
        code.extend_from_slice(&lane.to_le_bytes());
    }
}

fn push_i16x8(code: &mut Vec<u8>, lanes: [i16; 8]) {
    simd(code, 12);
    for lane in lanes {
        code.extend_from_slice(&lane.to_le_bytes());
    }
}

fn run_i32(code: &[u8]) -> i32 {
    let parsed = parse_module(&module(code)).expect("i32x4 ALU fixture parses");
    let mut instance = Instance::new(parsed).expect("i32x4 ALU fixture validates");
    match instance
        .invoke_export_values("run", &[])
        .expect("i32x4 ALU fixture executes")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected i32x4 ALU result: {other:?}"),
    }
}

fn unary_lane(input: [i32; 4], subopcode: u32, lane: u8) -> i32 {
    let mut code = Vec::new();
    push_i32x4(&mut code, input);
    simd(&mut code, subopcode);
    simd(&mut code, 27);
    code.push(lane);
    run_i32(&code)
}

fn binary_lane(lhs: [i32; 4], rhs: [i32; 4], subopcode: u32, lane: u8) -> i32 {
    let mut code = Vec::new();
    push_i32x4(&mut code, lhs);
    push_i32x4(&mut code, rhs);
    simd(&mut code, subopcode);
    simd(&mut code, 27);
    code.push(lane);
    run_i32(&code)
}

fn dot_lane(lhs: [i16; 8], rhs: [i16; 8], lane: u8) -> i32 {
    let mut code = Vec::new();
    push_i16x8(&mut code, lhs);
    push_i16x8(&mut code, rhs);
    simd(&mut code, 186);
    simd(&mut code, 27);
    code.push(lane);
    run_i32(&code)
}

#[test]
fn i32x4_abs_and_neg_wrap_minimum_lane() {
    let input = [i32::MIN, -7, 0, 9];

    assert_eq!(unary_lane(input, 160, 0), i32::MIN);
    assert_eq!(unary_lane(input, 160, 1), 7);
    assert_eq!(unary_lane(input, 161, 0), i32::MIN);
    assert_eq!(unary_lane(input, 161, 3), -9);
}

#[test]
fn i32x4_min_max_distinguish_signed_and_unsigned_ordering() {
    let lhs = [i32::MIN, -1, 5, 11];
    let rhs = [1, 2, 6, -12];

    assert_eq!(binary_lane(lhs, rhs, 182, 0), i32::MIN);
    assert_eq!(binary_lane(lhs, rhs, 183, 0), 1);
    assert_eq!(binary_lane(lhs, rhs, 184, 0), 1);
    assert_eq!(binary_lane(lhs, rhs, 185, 0), i32::MIN);
}

#[test]
fn i32x4_dot_i16x8_s_multiplies_and_pairwise_adds_with_wrapping() {
    let lhs = [-32_768, -32_768, 32_767, 32_767, -3, 4, -1, -2];
    let rhs = [-32_768, -32_768, 1, 1, 5, -6, -7, 8];

    assert_eq!(dot_lane(lhs, rhs, 0), i32::MIN);
    assert_eq!(dot_lane(lhs, rhs, 1), 65_534);
    assert_eq!(dot_lane(lhs, rhs, 2), -39);
    assert_eq!(dot_lane(lhs, rhs, 3), -9);
}

#[test]
fn i32x4_alu_closure_executes_inside_structured_control() {
    let mut code = vec![0x02, 0x7f];
    push_i16x8(&mut code, [-3, 4, 0, 0, 0, 0, 0, 0]);
    push_i16x8(&mut code, [5, -6, 0, 0, 0, 0, 0, 0]);
    simd(&mut code, 186);
    simd(&mut code, 27);
    code.push(0);
    code.push(0x0b);
    assert_eq!(run_i32(&code), -39);
}

#[test]
fn validator_rejects_i32x4_alu_type_confusion() {
    let mut unary = Vec::new();
    push_i32(&mut unary, 1);
    simd(&mut unary, 160);
    simd(&mut unary, 27);
    unary.push(0);

    let parsed = parse_module(&module(&unary)).expect("unary type-confusion fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));

    let mut binary = Vec::new();
    push_i32x4(&mut binary, [1; 4]);
    push_i32(&mut binary, 2);
    simd(&mut binary, 182);
    simd(&mut binary, 27);
    binary.push(0);

    let parsed = parse_module(&module(&binary)).expect("binary type-confusion fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}

#[test]
fn i64x2_abs_frontier_remains_fail_closed() {
    let mut code = Vec::new();
    push_i32x4(&mut code, [1, 2, 3, 4]);
    simd(&mut code, 192);
    simd(&mut code, 27);
    code.push(0);

    let parsed = parse_module(&module(&code)).expect("unsupported fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 192,
                ..
            }
        ))
    ));
}
