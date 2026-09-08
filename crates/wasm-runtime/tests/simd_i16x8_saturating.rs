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

fn push_i32_const(bytes: &mut Vec<u8>, mut value: i32) {
    bytes.push(0x41);
    loop {
        let byte = (value as u8) & 0x7f;
        value >>= 7;
        let sign_bit_set = byte & 0x40 != 0;
        let done = (value == 0 && !sign_bit_set) || (value == -1 && sign_bit_set);
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
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x00, 0x01, 0x7f]);
    push_section(&mut bytes, 3, &[0x01, 0x00]);
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

fn push_simd(instructions: &mut Vec<u8>, subopcode: u32) {
    instructions.push(0xfd);
    push_u32(instructions, subopcode);
}

fn push_splat(instructions: &mut Vec<u8>, value: i32) {
    push_i32_const(instructions, value);
    push_simd(instructions, 16); // i16x8.splat
}

fn push_extract_s(instructions: &mut Vec<u8>, lane: u8) {
    push_simd(instructions, 24); // i16x8.extract_lane_s
    instructions.push(lane);
}

fn push_extract_u(instructions: &mut Vec<u8>, lane: u8) {
    push_simd(instructions, 25); // i16x8.extract_lane_u
    instructions.push(lane);
}

fn run_i32(instructions: &[u8]) -> i32 {
    let parsed = parse_module(&module(instructions)).expect("i16x8 saturating fixture must parse");
    let mut instance = Instance::new(parsed).expect("i16x8 saturating fixture must validate");
    match instance
        .invoke_export_values("run", &[])
        .expect("i16x8 saturating fixture must execute")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected i16x8 saturating result: {other:?}"),
    }
}

#[test]
fn i16x8_signed_saturating_add_and_sub_clamp() {
    let mut add_hi = Vec::new();
    push_splat(&mut add_hi, 32_767);
    push_splat(&mut add_hi, 1);
    push_simd(&mut add_hi, 143); // i16x8.add_sat_s
    push_extract_s(&mut add_hi, 0);
    assert_eq!(run_i32(&add_hi), 32_767);

    let mut add_lo = Vec::new();
    push_splat(&mut add_lo, -32_768);
    push_splat(&mut add_lo, -1);
    push_simd(&mut add_lo, 143); // i16x8.add_sat_s
    push_extract_s(&mut add_lo, 7);
    assert_eq!(run_i32(&add_lo), -32_768);

    let mut sub_lo = Vec::new();
    push_splat(&mut sub_lo, -32_768);
    push_splat(&mut sub_lo, 1);
    push_simd(&mut sub_lo, 146); // i16x8.sub_sat_s
    push_extract_s(&mut sub_lo, 3);
    assert_eq!(run_i32(&sub_lo), -32_768);

    let mut sub_hi = Vec::new();
    push_splat(&mut sub_hi, 32_767);
    push_splat(&mut sub_hi, -1);
    push_simd(&mut sub_hi, 146); // i16x8.sub_sat_s
    push_extract_s(&mut sub_hi, 5);
    assert_eq!(run_i32(&sub_hi), 32_767);
}

#[test]
fn i16x8_unsigned_saturating_add_and_sub_clamp() {
    let mut add = Vec::new();
    push_splat(&mut add, 65_535);
    push_splat(&mut add, 1);
    push_simd(&mut add, 144); // i16x8.add_sat_u
    push_extract_u(&mut add, 2);
    assert_eq!(run_i32(&add), 65_535);

    let mut sub = Vec::new();
    push_splat(&mut sub, 0);
    push_splat(&mut sub, 1);
    push_simd(&mut sub, 147); // i16x8.sub_sat_u
    push_extract_u(&mut sub, 6);
    assert_eq!(run_i32(&sub), 0);
}

#[test]
fn i16x8_saturating_arithmetic_is_lane_independent_and_structured() {
    let mut instructions = vec![0x02, 0x7f]; // block (result i32)
    push_splat(&mut instructions, 100);
    push_i32_const(&mut instructions, 32_767);
    push_simd(&mut instructions, 26); // i16x8.replace_lane
    instructions.push(4);
    push_splat(&mut instructions, 10);
    push_simd(&mut instructions, 143); // i16x8.add_sat_s
    push_extract_s(&mut instructions, 4);
    instructions.push(0x0b); // end block
    assert_eq!(run_i32(&instructions), 32_767);

    let mut untouched = Vec::new();
    push_splat(&mut untouched, 100);
    push_i32_const(&mut untouched, 32_767);
    push_simd(&mut untouched, 26);
    untouched.push(4);
    push_splat(&mut untouched, 10);
    push_simd(&mut untouched, 143);
    push_extract_u(&mut untouched, 0);
    assert_eq!(run_i32(&untouched), 110);
}

#[test]
fn validator_rejects_i16x8_saturating_type_confusion() {
    let mut instructions = Vec::new();
    push_splat(&mut instructions, 1);
    push_i32_const(&mut instructions, 2); // wrong rhs type
    push_simd(&mut instructions, 143);
    push_extract_u(&mut instructions, 0);

    let parsed = parse_module(&module(&instructions)).expect("type-confusion fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}

#[test]
fn adjacent_i16x8_q15mulr_sat_s_remains_fail_closed() {
    let mut instructions = Vec::new();
    push_splat(&mut instructions, 1);
    push_splat(&mut instructions, 2);
    push_simd(&mut instructions, 148); // i16x8.q15mulr_sat_s remains outside this slice
    push_extract_u(&mut instructions, 0);

    let parsed = parse_module(&module(&instructions)).expect("unsupported-SIMD fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 148,
                ..
            }
        ))
    ));
}
