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
    let parsed = parse_module(&module(code)).expect("extmul fixture parses");
    let mut instance = Instance::new(parsed).expect("extmul fixture validates");
    match instance
        .invoke_export_values("run", &[])
        .expect("extmul fixture executes")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected extmul result: {other:?}"),
    }
}

fn i16_extmul_lane(
    lhs: [i8; 16],
    rhs: [i8; 16],
    subopcode: u32,
    lane: u8,
    signed_extract: bool,
) -> i32 {
    let mut code = Vec::new();
    push_i8x16(&mut code, lhs);
    push_i8x16(&mut code, rhs);
    simd(&mut code, subopcode);
    simd(&mut code, if signed_extract { 24 } else { 25 });
    code.push(lane);
    run_i32(&code)
}

fn i32_extmul_lane(lhs: [i16; 8], rhs: [i16; 8], subopcode: u32, lane: u8) -> i32 {
    let mut code = Vec::new();
    push_i16x8(&mut code, lhs);
    push_i16x8(&mut code, rhs);
    simd(&mut code, subopcode);
    simd(&mut code, 27);
    code.push(lane);
    run_i32(&code)
}

#[test]
fn i16x8_extmul_covers_low_high_signed_unsigned_products() {
    let lhs = [
        -128, 127, -2, 3, 4, 5, 6, 7, 8, -9, 10, -11, 12, -13, 14, -15,
    ];
    let rhs = [2, 2, -3, 4, 5, 6, 7, 8, -2, 3, -4, 5, -6, 7, -8, 9];

    assert_eq!(i16_extmul_lane(lhs, rhs, 156, 0, true), -256);
    assert_eq!(i16_extmul_lane(lhs, rhs, 157, 1, true), -27);
    assert_eq!(i16_extmul_lane(lhs, rhs, 158, 0, false), 256);
    assert_eq!(i16_extmul_lane(lhs, rhs, 159, 7, false), 2169);
}

#[test]
fn i32x4_extmul_covers_low_high_signed_unsigned_products() {
    let lhs = [-32_768, 32_767, -2, 3, 4, -5, 6, -7];
    let rhs = [2, 2, -3, 4, -5, 6, -7, 8];

    assert_eq!(i32_extmul_lane(lhs, rhs, 188, 0), -65_536);
    assert_eq!(i32_extmul_lane(lhs, rhs, 189, 1), -30);
    assert_eq!(i32_extmul_lane(lhs, rhs, 190, 0), 65_536);
    assert_eq!(i32_extmul_lane(lhs, rhs, 191, 3), 524_232);
}

#[test]
fn extmul_executes_inside_structured_control() {
    let mut code = vec![0x02, 0x7f];
    push_i8x16(&mut code, [-3; 16]);
    push_i8x16(&mut code, [7; 16]);
    simd(&mut code, 156);
    simd(&mut code, 24);
    code.push(0);
    code.push(0x0b);
    assert_eq!(run_i32(&code), -21);
}

#[test]
fn validator_rejects_extmul_type_confusion() {
    let mut code = Vec::new();
    push_i16x8(&mut code, [1; 8]);
    push_i32(&mut code, 2);
    simd(&mut code, 188);
    simd(&mut code, 27);
    code.push(0);

    let parsed = parse_module(&module(&code)).expect("type-confusion fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}
