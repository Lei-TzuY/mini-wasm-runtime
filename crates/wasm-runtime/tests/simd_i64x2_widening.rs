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
    section(&mut module, 1, &[1, 0x60, 0, 1, 0x7e]);
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

fn run_i64(code: &[u8]) -> i64 {
    let parsed = parse_module(&module(code)).expect("i64x2 widening fixture parses");
    let mut instance = Instance::new(parsed).expect("i64x2 widening fixture validates");
    match instance
        .invoke_export_values("run", &[])
        .expect("i64x2 widening fixture executes")
        .as_slice()
    {
        [Value::I64(value)] => *value,
        other => panic!("unexpected i64x2 widening result: {other:?}"),
    }
}

fn widened_lane(input: [i32; 4], subopcode: u32, lane: u8) -> i64 {
    let mut code = Vec::new();
    push_i32x4(&mut code, input);
    simd(&mut code, subopcode);
    simd(&mut code, 29);
    code.push(lane);
    run_i64(&code)
}

#[test]
fn i64x2_extend_i32x4_selects_low_high_and_signed_unsigned_lanes() {
    let lanes = [i32::MIN, -1, 2, i32::MAX];

    assert_eq!(widened_lane(lanes, 199, 0), i64::from(i32::MIN));
    assert_eq!(widened_lane(lanes, 199, 1), -1);
    assert_eq!(widened_lane(lanes, 200, 0), 2);
    assert_eq!(widened_lane(lanes, 200, 1), i64::from(i32::MAX));
    assert_eq!(widened_lane(lanes, 201, 0), 2_147_483_648);
    assert_eq!(widened_lane(lanes, 201, 1), 4_294_967_295);
    assert_eq!(widened_lane(lanes, 202, 0), 2);
    assert_eq!(widened_lane(lanes, 202, 1), 2_147_483_647);
}

#[test]
fn i64x2_widening_executes_inside_structured_control() {
    let mut code = vec![0x02, 0x7e];
    push_i32x4(&mut code, [-7, 1, 2, 3]);
    simd(&mut code, 199);
    simd(&mut code, 29);
    code.push(0);
    code.push(0x0b);
    assert_eq!(run_i64(&code), -7);
}

#[test]
fn validator_rejects_i64x2_widening_type_confusion() {
    let mut code = Vec::new();
    push_i32(&mut code, 1);
    simd(&mut code, 199);
    simd(&mut code, 29);
    code.push(0);

    let parsed = parse_module(&module(&code)).expect("type-confusion fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}
