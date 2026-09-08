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

fn push_v128_bytes(instructions: &mut Vec<u8>, lanes: [u8; 16]) {
    push_simd(instructions, 12); // v128.const
    instructions.extend_from_slice(&lanes);
}

fn run_i32(instructions: &[u8]) -> i32 {
    let parsed = parse_module(&module(instructions)).expect("i8x16 comparison fixture must parse");
    let mut instance = Instance::new(parsed).expect("i8x16 comparison fixture must validate");
    match instance
        .invoke_export_values("run", &[])
        .expect("i8x16 comparison fixture must execute")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected i8x16 comparison result: {other:?}"),
    }
}

const LHS: [u8; 16] = [
    0x80, 0x7f, 0xff, 0x00, 0x01, 0x80, 0xfe, 0x64, 0xc8, 0x32, 0x00, 0xff, 0x7f, 0x81, 0x02, 0x02,
];
const RHS: [u8; 16] = [
    0x7f, 0x80, 0x01, 0x00, 0x02, 0x7f, 0xff, 0x64, 0x64, 0xc8, 0xff, 0x00, 0x7f, 0x80, 0x03, 0x01,
];

fn comparison_lane(subopcode: u32, lane: u8) -> i32 {
    let mut instructions = Vec::new();
    push_v128_bytes(&mut instructions, LHS);
    push_v128_bytes(&mut instructions, RHS);
    push_simd(&mut instructions, subopcode);
    push_simd(&mut instructions, 22); // i8x16.extract_lane_u
    instructions.push(lane);
    run_i32(&instructions)
}

#[test]
fn i8x16_comparisons_produce_canonical_byte_masks() {
    let cases = [
        (35, 3, 255, 0, 0), // i8x16.eq: equal lane 3, unequal lane 0
        (36, 0, 255, 3, 0), // i8x16.ne
        (37, 0, 255, 1, 0), // i8x16.lt_s: -128 < 127, 127 !< -128
        (38, 1, 255, 0, 0), // i8x16.lt_u: 127 < 128, 128 !< 127
        (39, 1, 255, 0, 0), // i8x16.gt_s: 127 > -128
        (40, 0, 255, 1, 0), // i8x16.gt_u: 128 > 127
        (41, 0, 255, 1, 0), // i8x16.le_s
        (42, 1, 255, 0, 0), // i8x16.le_u
        (43, 1, 255, 0, 0), // i8x16.ge_s
        (44, 0, 255, 1, 0), // i8x16.ge_u
    ];

    for (subopcode, true_lane, true_mask, false_lane, false_mask) in cases {
        assert_eq!(
            comparison_lane(subopcode, true_lane),
            true_mask,
            "subopcode {subopcode} true lane"
        );
        assert_eq!(
            comparison_lane(subopcode, false_lane),
            false_mask,
            "subopcode {subopcode} false lane"
        );
    }
}

#[test]
fn i8x16_comparison_executes_inside_structured_control() {
    let mut instructions = vec![0x02, 0x7f]; // block (result i32)
    push_v128_bytes(&mut instructions, LHS);
    push_v128_bytes(&mut instructions, RHS);
    push_simd(&mut instructions, 35); // i8x16.eq
    push_simd(&mut instructions, 22); // i8x16.extract_lane_u
    instructions.push(3);
    instructions.push(0x0b); // end block
    assert_eq!(run_i32(&instructions), 255);
}

#[test]
fn validator_rejects_i8x16_comparison_type_confusion() {
    let mut instructions = Vec::new();
    push_v128_bytes(&mut instructions, LHS);
    instructions.extend_from_slice(&[0x41, 0x01]); // i32.const 1 where rhs v128 is required
    push_simd(&mut instructions, 35); // i8x16.eq
    push_simd(&mut instructions, 22); // i8x16.extract_lane_u
    instructions.push(0);

    let parsed = parse_module(&module(&instructions)).expect("type-confusion fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}
