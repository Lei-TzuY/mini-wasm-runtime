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
    let parsed = parse_module(&module(instructions)).expect("i16x8 arithmetic fixture must parse");
    let mut instance = Instance::new(parsed).expect("i16x8 arithmetic fixture must validate");
    match instance
        .invoke_export_values("run", &[])
        .expect("i16x8 arithmetic fixture must execute")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected i16x8 arithmetic result: {other:?}"),
    }
}

#[test]
fn i16x8_add_sub_mul_wrap_per_lane() {
    let mut add = Vec::new();
    push_splat(&mut add, 32_767);
    push_splat(&mut add, 1);
    push_simd(&mut add, 142); // i16x8.add
    push_extract_s(&mut add, 4);
    assert_eq!(run_i32(&add), -32_768);

    let mut sub = Vec::new();
    push_splat(&mut sub, -32_768);
    push_splat(&mut sub, 1);
    push_simd(&mut sub, 145); // i16x8.sub
    push_extract_s(&mut sub, 6);
    assert_eq!(run_i32(&sub), 32_767);

    let mut mul = Vec::new();
    push_splat(&mut mul, 300);
    push_splat(&mut mul, 300);
    push_simd(&mut mul, 149); // i16x8.mul
    push_extract_u(&mut mul, 2);
    assert_eq!(run_i32(&mul), 24_464);
}

#[test]
fn i16x8_arithmetic_keeps_lanes_independent() {
    let mut instructions = Vec::new();
    push_splat(&mut instructions, 10);
    push_i32_const(&mut instructions, 100);
    push_simd(&mut instructions, 26); // i16x8.replace_lane 3
    instructions.push(3);
    push_splat(&mut instructions, 1);
    push_i32_const(&mut instructions, 2);
    push_simd(&mut instructions, 26); // i16x8.replace_lane 3
    instructions.push(3);
    push_simd(&mut instructions, 142); // i16x8.add
    push_extract_u(&mut instructions, 3);
    assert_eq!(run_i32(&instructions), 102);

    let mut untouched = Vec::new();
    push_splat(&mut untouched, 10);
    push_i32_const(&mut untouched, 100);
    push_simd(&mut untouched, 26);
    untouched.push(3);
    push_splat(&mut untouched, 1);
    push_i32_const(&mut untouched, 2);
    push_simd(&mut untouched, 26);
    untouched.push(3);
    push_simd(&mut untouched, 142);
    push_extract_u(&mut untouched, 0);
    assert_eq!(run_i32(&untouched), 11);
}

#[test]
fn i16x8_arithmetic_executes_inside_structured_control() {
    let mut instructions = vec![0x02, 0x7f]; // block (result i32)
    push_splat(&mut instructions, 200);
    push_splat(&mut instructions, 3);
    push_simd(&mut instructions, 149); // i16x8.mul
    push_extract_u(&mut instructions, 7);
    instructions.push(0x0b); // end block
    assert_eq!(run_i32(&instructions), 600);
}

#[test]
fn validator_rejects_i16x8_arithmetic_type_confusion() {
    let mut instructions = Vec::new();
    push_splat(&mut instructions, 1);
    push_i32_const(&mut instructions, 2); // wrong rhs type
    push_simd(&mut instructions, 142); // i16x8.add requires two v128 values
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
fn adjacent_i16x8_saturating_add_remains_fail_closed() {
    let mut instructions = Vec::new();
    push_splat(&mut instructions, 1);
    push_splat(&mut instructions, 2);
    push_simd(&mut instructions, 143); // i16x8.add_sat_s remains outside this slice
    push_extract_u(&mut instructions, 0);

    let parsed = parse_module(&module(&instructions)).expect("unsupported-SIMD fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 143,
                ..
            }
        ))
    ));
}
