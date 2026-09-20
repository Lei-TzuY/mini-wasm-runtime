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

fn push_i64(bytes: &mut Vec<u8>, mut value: i64) {
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

fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn module(result_type: u8, instructions: &[u8]) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 0, 1, result_type]);
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

fn push_i64x2(code: &mut Vec<u8>, lanes: [i64; 2]) {
    simd(code, 12);
    for lane in lanes {
        code.extend_from_slice(&lane.to_le_bytes());
    }
}

fn run_i64(code: &[u8]) -> i64 {
    let parsed = parse_module(&module(0x7e, code)).expect("i64x2 fixture parses");
    let mut instance = Instance::new(parsed).expect("i64x2 fixture validates");
    match instance
        .invoke_export_values("run", &[])
        .expect("i64x2 fixture executes")
        .as_slice()
    {
        [Value::I64(value)] => *value,
        other => panic!("unexpected i64x2 result: {other:?}"),
    }
}

fn run_i32(code: &[u8]) -> i32 {
    let parsed = parse_module(&module(0x7f, code)).expect("i64x2 reduction fixture parses");
    let mut instance = Instance::new(parsed).expect("i64x2 reduction fixture validates");
    match instance
        .invoke_export_values("run", &[])
        .expect("i64x2 reduction fixture executes")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected i64x2 reduction result: {other:?}"),
    }
}

fn unary_lane(input: [i64; 2], subopcode: u32, lane: u8) -> i64 {
    let mut code = Vec::new();
    push_i64x2(&mut code, input);
    simd(&mut code, subopcode);
    simd(&mut code, 29);
    code.push(lane);
    run_i64(&code)
}

fn reduction(input: [i64; 2], subopcode: u32) -> i32 {
    let mut code = Vec::new();
    push_i64x2(&mut code, input);
    simd(&mut code, subopcode);
    run_i32(&code)
}

#[test]
fn i64x2_abs_and_neg_are_wrapping_per_lane() {
    let input = [i64::MIN, -7];

    assert_eq!(unary_lane(input, 192, 0), i64::MIN);
    assert_eq!(unary_lane(input, 192, 1), 7);
    assert_eq!(unary_lane(input, 193, 0), i64::MIN);
    assert_eq!(unary_lane(input, 193, 1), 7);
}

#[test]
fn i64x2_all_true_and_bitmask_follow_lane_semantics() {
    assert_eq!(reduction([1, -2], 195), 1);
    assert_eq!(reduction([1, 0], 195), 0);
    assert_eq!(reduction([-1, i64::MAX], 196), 0b01);
    assert_eq!(reduction([-1, i64::MIN], 196), 0b11);
}

#[test]
fn i64x2_unary_and_reductions_execute_inside_structured_control() {
    for subopcode in [192, 193] {
        let mut code = vec![0x02, 0x40];
        push_i64x2(&mut code, [-9, i64::MIN]);
        simd(&mut code, subopcode);
        code.push(0x1a);
        code.push(0x0b);
        code.extend_from_slice(&[0x41, 0]);
        assert_eq!(run_i32(&code), 0);
    }

    for subopcode in [195, 196] {
        let mut code = vec![0x02, 0x40];
        push_i64x2(&mut code, [-1, 2]);
        simd(&mut code, subopcode);
        code.push(0x1a);
        code.push(0x0b);
        code.extend_from_slice(&[0x41, 0]);
        assert_eq!(run_i32(&code), 0);
    }
}

#[test]
fn validator_rejects_i64x2_unary_and_reduction_type_confusion() {
    for subopcode in [192, 193, 195, 196] {
        let mut code = Vec::new();
        push_i64(&mut code, 1);
        simd(&mut code, subopcode);
        if matches!(subopcode, 192 | 193) {
            simd(&mut code, 29);
            code.push(0);
        }

        let result_type = if matches!(subopcode, 192 | 193) {
            0x7e
        } else {
            0x7f
        };
        let parsed =
            parse_module(&module(result_type, &code)).expect("type-confusion fixture parses");
        assert!(matches!(
            Instance::new(parsed),
            Err(RuntimeError::Validation(
                ValidationError::TypeMismatch { .. }
            ))
        ));
    }
}
