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

fn push_v128_const(instructions: &mut Vec<u8>, bytes: [u8; 16]) {
    instructions.extend_from_slice(&[0xfd, 0x0c]);
    instructions.extend_from_slice(&bytes);
}

fn lane_pattern(bytes: [u8; 4]) -> [u8; 16] {
    let mut vector = [0u8; 16];
    for lane in vector.chunks_exact_mut(4) {
        lane.copy_from_slice(&bytes);
    }
    vector
}

fn run_lane0(instructions: Vec<u8>) -> i32 {
    let bytes = module(&instructions);
    let parsed = parse_module(&bytes).expect("SIMD bitwise fixture must parse");
    let mut instance = Instance::new(parsed).expect("SIMD bitwise fixture must validate");
    let result = instance
        .invoke_export_values("run", &[])
        .expect("SIMD bitwise fixture must execute");
    assert_eq!(result.len(), 1);
    match result[0] {
        Value::I32(value) => value,
        ref other => panic!("unexpected SIMD bitwise result: {other:?}"),
    }
}

#[test]
fn v128_not_and_binary_bitwise_ops_are_bit_exact() {
    let lhs = lane_pattern([0xf0, 0x0f, 0xaa, 0x55]);
    let rhs = lane_pattern([0xcc, 0x33, 0xff, 0x00]);

    let mut not = Vec::new();
    push_v128_const(&mut not, lhs);
    not.extend_from_slice(&[0xfd, 0x4d, 0xfd, 0x1b, 0x00]);
    assert_eq!(run_lane0(not), 0xaa55_f00fu32 as i32);

    for (subopcode, expected) in [
        (0x4e, 0x00aa_03c0u32),
        (0x4f, 0x5500_0c30u32),
        (0x50, 0x55ff_3ffcu32),
        (0x51, 0x5555_3c3cu32),
    ] {
        let mut instructions = Vec::new();
        push_v128_const(&mut instructions, lhs);
        push_v128_const(&mut instructions, rhs);
        instructions.extend_from_slice(&[0xfd, subopcode, 0xfd, 0x1b, 0x00]);
        assert_eq!(run_lane0(instructions), expected as i32);
    }
}

#[test]
fn v128_bitselect_executes_inside_structured_control() {
    let mut instructions = vec![0x02, 0x7f];
    push_v128_const(&mut instructions, [0xaa; 16]);
    push_v128_const(&mut instructions, [0x55; 16]);
    push_v128_const(&mut instructions, [0xf0; 16]);
    instructions.extend_from_slice(&[
        0xfd, 0x52, // v128.bitselect
        0xfd, 0x1b, 0x00, // i32x4.extract_lane 0
        0x0b, // end block
    ]);
    assert_eq!(run_lane0(instructions), 0xa5a5_a5a5u32 as i32);
}

#[test]
fn v128_any_true_returns_canonical_boolean() {
    let mut zero = Vec::new();
    push_v128_const(&mut zero, [0; 16]);
    zero.extend_from_slice(&[0xfd, 0x53]);
    assert_eq!(run_lane0(zero), 0);

    let mut nonzero_bytes = [0u8; 16];
    nonzero_bytes[15] = 1;
    let mut nonzero = Vec::new();
    push_v128_const(&mut nonzero, nonzero_bytes);
    nonzero.extend_from_slice(&[0xfd, 0x53]);
    assert_eq!(run_lane0(nonzero), 1);
}

#[test]
fn validator_rejects_v128_bitwise_type_confusion() {
    let bytes = module(&[
        0x41, 0x00, // i32.const 0
        0xfd, 0x4d, // v128.not requires v128
        0x41, 0x00, // unreachable result if validation were skipped
    ]);
    let parsed = parse_module(&bytes).expect("SIMD type-confusion fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}
