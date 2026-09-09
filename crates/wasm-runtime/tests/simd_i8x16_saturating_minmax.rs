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
    push_simd(instructions, 15); // i8x16.splat
}

fn push_extract_s(instructions: &mut Vec<u8>, lane: u8) {
    push_simd(instructions, 21); // i8x16.extract_lane_s
    instructions.push(lane);
}

fn push_extract_u(instructions: &mut Vec<u8>, lane: u8) {
    push_simd(instructions, 22); // i8x16.extract_lane_u
    instructions.push(lane);
}

fn run_i32(instructions: &[u8]) -> i32 {
    let parsed = parse_module(&module(instructions)).expect("i8x16 ALU fixture must parse");
    let mut instance = Instance::new(parsed).expect("i8x16 ALU fixture must validate");
    match instance
        .invoke_export_values("run", &[])
        .expect("i8x16 ALU fixture must execute")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected i8x16 ALU result: {other:?}"),
    }
}

#[test]
fn i8x16_signed_saturating_add_and_sub_clamp() {
    let mut add_hi = Vec::new();
    push_splat(&mut add_hi, 127);
    push_splat(&mut add_hi, 1);
    push_simd(&mut add_hi, 111);
    push_extract_s(&mut add_hi, 0);
    assert_eq!(run_i32(&add_hi), 127);

    let mut add_lo = Vec::new();
    push_splat(&mut add_lo, -128);
    push_splat(&mut add_lo, -1);
    push_simd(&mut add_lo, 111);
    push_extract_s(&mut add_lo, 15);
    assert_eq!(run_i32(&add_lo), -128);

    let mut sub_lo = Vec::new();
    push_splat(&mut sub_lo, -128);
    push_splat(&mut sub_lo, 1);
    push_simd(&mut sub_lo, 114);
    push_extract_s(&mut sub_lo, 7);
    assert_eq!(run_i32(&sub_lo), -128);

    let mut sub_hi = Vec::new();
    push_splat(&mut sub_hi, 127);
    push_splat(&mut sub_hi, -1);
    push_simd(&mut sub_hi, 114);
    push_extract_s(&mut sub_hi, 4);
    assert_eq!(run_i32(&sub_hi), 127);
}

#[test]
fn i8x16_unsigned_saturating_add_and_sub_clamp() {
    let mut add = Vec::new();
    push_splat(&mut add, 255);
    push_splat(&mut add, 1);
    push_simd(&mut add, 112);
    push_extract_u(&mut add, 3);
    assert_eq!(run_i32(&add), 255);

    let mut sub = Vec::new();
    push_splat(&mut sub, 0);
    push_splat(&mut sub, 1);
    push_simd(&mut sub, 115);
    push_extract_u(&mut sub, 12);
    assert_eq!(run_i32(&sub), 0);
}

#[test]
fn i8x16_min_max_and_average_observe_lane_signedness() {
    let cases = [
        (118, true, -1, 1, -1),
        (120, true, -1, 1, 1),
        (119, false, 255, 1, 1),
        (121, false, 255, 1, 255),
        (123, false, 10, 13, 12),
    ];

    for (opcode, signed, lhs, rhs, expected) in cases {
        let mut instructions = Vec::new();
        push_splat(&mut instructions, lhs);
        push_splat(&mut instructions, rhs);
        push_simd(&mut instructions, opcode);
        if signed {
            push_extract_s(&mut instructions, 5);
        } else {
            push_extract_u(&mut instructions, 5);
        }
        assert_eq!(run_i32(&instructions), expected, "subopcode {opcode}");
    }
}

#[test]
fn i8x16_alu_executes_inside_structured_control() {
    let mut instructions = vec![0x02, 0x7f]; // block (result i32)
    push_splat(&mut instructions, 100);
    push_splat(&mut instructions, 40);
    push_simd(&mut instructions, 120); // i8x16.max_s
    push_extract_s(&mut instructions, 9);
    instructions.push(0x0b);
    assert_eq!(run_i32(&instructions), 100);
}

#[test]
fn validator_rejects_i8x16_alu_type_confusion() {
    let mut instructions = Vec::new();
    push_splat(&mut instructions, 1);
    push_i32_const(&mut instructions, 2); // wrong rhs type
    push_simd(&mut instructions, 111);
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
fn adjacent_i8x16_wrapping_add_remains_fail_closed() {
    let mut instructions = Vec::new();
    push_splat(&mut instructions, 1);
    push_splat(&mut instructions, 2);
    push_simd(&mut instructions, 110); // i8x16.add remains outside this slice
    push_extract_u(&mut instructions, 0);

    let parsed = parse_module(&module(&instructions)).expect("unsupported-SIMD fixture must parse");
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
