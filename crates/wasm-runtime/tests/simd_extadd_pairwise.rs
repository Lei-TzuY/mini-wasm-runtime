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

fn push_i8x16(code: &mut Vec<u8>, lanes: [i8; 16]) {
    simd(code, 12);
    code.extend(lanes.map(|lane| lane as u8));
}

fn push_i16x8(code: &mut Vec<u8>, lanes: [i16; 8]) {
    simd(code, 12);
    for lane in lanes {
        code.extend_from_slice(&lane.to_le_bytes());
    }
}

fn run_i32(code: &[u8]) -> i32 {
    let parsed = parse_module(&module(code)).expect("pairwise fixture parses");
    let mut instance = Instance::new(parsed).expect("pairwise fixture validates");
    match instance
        .invoke_export_values("run", &[])
        .expect("pairwise fixture executes")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected pairwise result: {other:?}"),
    }
}

fn i16_pairwise_lane(input: [i8; 16], subopcode: u32, lane: u8, signed: bool) -> i32 {
    let mut code = Vec::new();
    push_i8x16(&mut code, input);
    simd(&mut code, subopcode);
    simd(&mut code, if signed { 24 } else { 25 });
    code.push(lane);
    run_i32(&code)
}

fn i32_pairwise_lane(input: [i16; 8], subopcode: u32, lane: u8) -> i32 {
    let mut code = Vec::new();
    push_i16x8(&mut code, input);
    simd(&mut code, subopcode);
    simd(&mut code, 27);
    code.push(lane);
    run_i32(&code)
}

#[test]
fn i16x8_pairwise_add_extends_adjacent_i8_lanes() {
    let input = [
        -128, -128, 127, 127, -5, -6, 10, 20, -1, 1, 100, -100, 50, 60, -70, 80,
    ];

    assert_eq!(i16_pairwise_lane(input, 124, 0, true), -256);
    assert_eq!(i16_pairwise_lane(input, 124, 2, true), -11);
    assert_eq!(i16_pairwise_lane(input, 125, 0, false), 256);
    assert_eq!(i16_pairwise_lane(input, 125, 2, false), 501);
}

#[test]
fn i32x4_pairwise_add_extends_adjacent_i16_lanes() {
    let input = [-32_768, -32_768, 32_767, 32_767, -3_000, 4_000, -1, -2];

    assert_eq!(i32_pairwise_lane(input, 126, 0), -65_536);
    assert_eq!(i32_pairwise_lane(input, 126, 2), 1_000);
    assert_eq!(i32_pairwise_lane(input, 127, 0), 65_536);
    assert_eq!(i32_pairwise_lane(input, 127, 2), 66_536);
}

#[test]
fn pairwise_add_executes_inside_structured_control() {
    let mut code = vec![0x02, 0x7f];
    push_i8x16(&mut code, [-7, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    simd(&mut code, 124);
    simd(&mut code, 24);
    code.push(0);
    code.push(0x0b);
    assert_eq!(run_i32(&code), -5);
}

#[test]
fn validator_rejects_pairwise_type_confusion() {
    let mut code = Vec::new();
    push_i32(&mut code, 1);
    simd(&mut code, 124);
    simd(&mut code, 24);
    code.push(0);

    let parsed = parse_module(&module(&code)).expect("type-confusion fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}

#[test]
fn i8x16_wrapping_add_frontier_remains_fail_closed() {
    let mut code = Vec::new();
    push_i8x16(&mut code, [1; 16]);
    push_i8x16(&mut code, [2; 16]);
    simd(&mut code, 110);
    simd(&mut code, 21);
    code.push(0);

    let parsed = parse_module(&module(&code)).expect("unsupported fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 110,
                ..
            }
        ))
    ));
}
