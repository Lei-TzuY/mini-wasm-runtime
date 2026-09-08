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

fn push_v128_i32x4(instructions: &mut Vec<u8>, lanes: [i32; 4]) {
    push_simd(instructions, 12);
    for lane in lanes {
        instructions.extend_from_slice(&lane.to_le_bytes());
    }
}

fn run_i32(instructions: &[u8]) -> i32 {
    let bytes = module(instructions);
    let parsed = parse_module(&bytes).expect("SIMD reduction fixture must parse");
    let mut instance = Instance::new(parsed).expect("SIMD reduction fixture must validate");
    match instance
        .invoke_export_values("run", &[])
        .expect("SIMD reduction fixture must execute")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected SIMD reduction result: {other:?}"),
    }
}

#[test]
fn i32x4_all_true_returns_one_for_four_nonzero_lanes_inside_structured_control() {
    let mut instructions = vec![0x02, 0x7f]; // block (result i32)
    push_v128_i32x4(&mut instructions, [1, -2, 3, i32::MIN]);
    push_simd(&mut instructions, 0xa3); // i32x4.all_true
    instructions.push(0x0b); // end block
    assert_eq!(run_i32(&instructions), 1);
}

#[test]
fn i32x4_all_true_returns_zero_when_any_lane_is_zero() {
    let mut instructions = Vec::new();
    push_v128_i32x4(&mut instructions, [1, 0, -3, 4]);
    push_simd(&mut instructions, 0xa3); // i32x4.all_true
    assert_eq!(run_i32(&instructions), 0);
}

#[test]
fn i32x4_bitmask_extracts_lane_sign_bits_in_lane_order() {
    let mut instructions = vec![0x02, 0x7f]; // block (result i32)
    push_v128_i32x4(&mut instructions, [-1, 0, i32::MIN, i32::MAX]);
    push_simd(&mut instructions, 0xa4); // i32x4.bitmask
    instructions.push(0x0b); // end block
    assert_eq!(run_i32(&instructions), 0b0101);
}

#[test]
fn validator_rejects_i32x4_all_true_type_confusion() {
    let mut instructions = vec![0x41, 0x01]; // i32.const 1 where v128 is required
    push_simd(&mut instructions, 0xa3); // i32x4.all_true

    let parsed = parse_module(&module(&instructions)).expect("type-confusion fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}
