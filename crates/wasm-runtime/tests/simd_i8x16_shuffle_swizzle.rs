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

fn push_v128_const(code: &mut Vec<u8>, lanes: [u8; 16]) {
    code.extend_from_slice(&[0xfd, 0x0c]);
    code.extend_from_slice(&lanes);
}

fn run_i32(instructions: &[u8]) -> i32 {
    let parsed =
        parse_module(&module(instructions)).expect("SIMD shuffle/swizzle fixture must parse");
    let mut instance = Instance::new(parsed).expect("SIMD shuffle/swizzle fixture must validate");
    match instance
        .invoke_export_values("run", &[])
        .expect("SIMD shuffle/swizzle fixture must execute")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected SIMD shuffle/swizzle result: {other:?}"),
    }
}

fn lhs() -> [u8; 16] {
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
}

fn rhs() -> [u8; 16] {
    [
        100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115,
    ]
}

#[test]
fn i8x16_shuffle_selects_across_both_inputs_and_repeats_lanes() {
    let mut cross = Vec::new();
    push_v128_const(&mut cross, lhs());
    push_v128_const(&mut cross, rhs());
    cross.extend_from_slice(&[
        0xfd, 0x0d, // i8x16.shuffle
        0, 17, 2, 19, 4, 21, 6, 23, 8, 25, 10, 27, 12, 29, 14, 31, 0xfd, 0x16,
        0x01, // i8x16.extract_lane_u 1
    ]);
    assert_eq!(run_i32(&cross), 101);

    let mut repeated = Vec::new();
    push_v128_const(&mut repeated, lhs());
    push_v128_const(&mut repeated, rhs());
    repeated.extend_from_slice(&[0xfd, 0x0d]);
    repeated.extend_from_slice(&[31; 16]);
    repeated.extend_from_slice(&[0xfd, 0x16, 0x07]);
    assert_eq!(run_i32(&repeated), 115);
}

#[test]
fn i8x16_swizzle_uses_dynamic_indices_and_zeroes_out_of_range_lanes() {
    let table = [
        10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    ];

    let mut in_range = Vec::new();
    push_v128_const(&mut in_range, table);
    push_v128_const(
        &mut in_range,
        [15, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14],
    );
    in_range.extend_from_slice(&[
        0xfd, 0x0e, // i8x16.swizzle
        0xfd, 0x16, 0x00, // i8x16.extract_lane_u 0
    ]);
    assert_eq!(run_i32(&in_range), 25);

    let mut out_of_range = Vec::new();
    push_v128_const(&mut out_of_range, table);
    push_v128_const(
        &mut out_of_range,
        [16, 255, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13],
    );
    out_of_range.extend_from_slice(&[
        0xfd, 0x0e, // i8x16.swizzle
        0xfd, 0x16, 0x00, // i8x16.extract_lane_u 0
    ]);
    assert_eq!(run_i32(&out_of_range), 0);
}

#[test]
fn i8x16_shuffle_executes_inside_structured_control() {
    let mut code = vec![0x02, 0x7f]; // block (result i32)
    push_v128_const(&mut code, lhs());
    push_v128_const(&mut code, rhs());
    code.extend_from_slice(&[
        0xfd, 0x0d, // i8x16.shuffle
        16, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 0xfd, 0x16,
        0x00, // i8x16.extract_lane_u 0
        0x0b, // end block
    ]);
    assert_eq!(run_i32(&code), 100);
}

#[test]
fn validator_rejects_out_of_range_i8x16_shuffle_lane() {
    let mut code = Vec::new();
    push_v128_const(&mut code, lhs());
    push_v128_const(&mut code, rhs());
    code.extend_from_slice(&[
        0xfd, 0x0d, // i8x16.shuffle
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 32, 0xfd, 0x16, 0x00,
    ]);
    let bytes = module(&code);
    let parsed = parse_module(&bytes).expect("invalid-shuffle-lane fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::MalformedImmediate { .. }
        ))
    ));
}

#[test]
fn validator_rejects_i8x16_swizzle_type_confusion() {
    let mut code = Vec::new();
    push_v128_const(&mut code, lhs());
    code.extend_from_slice(&[
        0x41, 0x00, // i32.const 0 where swizzle requires a v128 index vector
        0xfd, 0x0e, // i8x16.swizzle
        0xfd, 0x16, 0x00,
    ]);
    let bytes = module(&code);
    let parsed = parse_module(&bytes).expect("swizzle type-confusion fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}

#[test]
fn validator_rejects_i8x16_shuffle_type_confusion() {
    let mut code = Vec::new();
    push_v128_const(&mut code, lhs());
    code.extend_from_slice(&[
        0x41, 0x00, // i32.const 0 where shuffle requires a second v128
        0xfd, 0x0d, // i8x16.shuffle
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 0xfd, 0x16, 0x00,
    ]);
    let bytes = module(&code);
    let parsed = parse_module(&bytes).expect("shuffle type-confusion fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}
