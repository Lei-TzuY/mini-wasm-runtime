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
    let parsed = parse_module(&bytes).expect("SIMD comparison fixture must parse");
    let mut instance = Instance::new(parsed).expect("SIMD comparison fixture must validate");
    match instance
        .invoke_export_values("run", &[])
        .expect("SIMD comparison fixture must execute")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected SIMD comparison result: {other:?}"),
    }
}

fn comparison_bitmask(subopcode: u32) -> i32 {
    let lhs = [-1, 0, 5, i32::MIN];
    let rhs = [0, 0, 4, i32::MAX];
    let mut instructions = Vec::new();
    push_v128_i32x4(&mut instructions, lhs);
    push_v128_i32x4(&mut instructions, rhs);
    push_simd(&mut instructions, subopcode);
    push_simd(&mut instructions, 164); // i32x4.bitmask
    run_i32(&instructions)
}

#[test]
fn i32x4_comparisons_produce_canonical_lane_masks() {
    let cases = [
        (55, 0b0010), // i32x4.eq
        (56, 0b1101), // i32x4.ne
        (57, 0b1001), // i32x4.lt_s
        (58, 0b0000), // i32x4.lt_u
        (59, 0b0100), // i32x4.gt_s
        (60, 0b1101), // i32x4.gt_u
        (61, 0b1011), // i32x4.le_s
        (62, 0b0010), // i32x4.le_u
        (63, 0b0110), // i32x4.ge_s
        (64, 0b1111), // i32x4.ge_u
    ];

    for (subopcode, expected) in cases {
        assert_eq!(
            comparison_bitmask(subopcode),
            expected,
            "subopcode {subopcode}"
        );
    }
}

#[test]
fn i32x4_comparison_executes_inside_structured_control() {
    let mut instructions = vec![0x02, 0x7f]; // block (result i32)
    push_v128_i32x4(&mut instructions, [1, 2, 3, 4]);
    push_v128_i32x4(&mut instructions, [1, 0, 3, 5]);
    push_simd(&mut instructions, 55); // i32x4.eq
    push_simd(&mut instructions, 164); // i32x4.bitmask
    instructions.push(0x0b); // end block
    assert_eq!(run_i32(&instructions), 0b0101);
}

#[test]
fn validator_rejects_i32x4_comparison_type_confusion() {
    let mut instructions = Vec::new();
    push_v128_i32x4(&mut instructions, [1, 2, 3, 4]);
    instructions.extend_from_slice(&[0x41, 0x01]); // i32.const 1 where rhs v128 is required
    push_simd(&mut instructions, 55); // i32x4.eq
    push_simd(&mut instructions, 164); // i32x4.bitmask

    let parsed = parse_module(&module(&instructions)).expect("type-confusion fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}
