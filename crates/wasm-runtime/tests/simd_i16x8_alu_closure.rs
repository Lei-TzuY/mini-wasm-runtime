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

fn push_lanes(instructions: &mut Vec<u8>, lanes: [i32; 8]) {
    push_splat(instructions, 0);
    for (lane, value) in lanes.into_iter().enumerate() {
        push_i32_const(instructions, value);
        push_simd(instructions, 26); // i16x8.replace_lane
        instructions.push(lane as u8);
    }
}

fn run_i32(instructions: &[u8]) -> i32 {
    let parsed = parse_module(&module(instructions)).expect("i16x8 ALU fixture must parse");
    let mut instance = Instance::new(parsed).expect("i16x8 ALU fixture must validate");
    match instance
        .invoke_export_values("run", &[])
        .expect("i16x8 ALU fixture must execute")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected i16x8 ALU result: {other:?}"),
    }
}

#[test]
fn i16x8_abs_neg_and_q15mulr_cover_signed_boundaries() {
    let mut abs = Vec::new();
    push_splat(&mut abs, -123);
    push_simd(&mut abs, 128); // i16x8.abs
    push_extract_s(&mut abs, 6);
    assert_eq!(run_i32(&abs), 123);

    let mut abs_min = Vec::new();
    push_splat(&mut abs_min, -32_768);
    push_simd(&mut abs_min, 128);
    push_extract_s(&mut abs_min, 0);
    assert_eq!(run_i32(&abs_min), -32_768);

    let mut neg = Vec::new();
    push_splat(&mut neg, 123);
    push_simd(&mut neg, 129); // i16x8.neg
    push_extract_s(&mut neg, 2);
    assert_eq!(run_i32(&neg), -123);

    let mut q15 = Vec::new();
    push_splat(&mut q15, -32_768);
    push_splat(&mut q15, -32_768);
    push_simd(&mut q15, 130); // i16x8.q15mulr_sat_s
    push_extract_s(&mut q15, 7);
    assert_eq!(run_i32(&q15), 32_767);

    let mut q15_half = Vec::new();
    push_splat(&mut q15_half, 16_384);
    push_splat(&mut q15_half, 16_384);
    push_simd(&mut q15_half, 130);
    push_extract_s(&mut q15_half, 4);
    assert_eq!(run_i32(&q15_half), 8_192);
}

#[test]
fn i16x8_reductions_return_canonical_scalar_results() {
    let mut all_true = Vec::new();
    push_lanes(&mut all_true, [1, -1, 2, -2, 3, -3, 4, -4]);
    push_simd(&mut all_true, 131); // i16x8.all_true
    assert_eq!(run_i32(&all_true), 1);

    let mut not_all_true = Vec::new();
    push_lanes(&mut not_all_true, [1, -1, 2, 0, 3, -3, 4, -4]);
    push_simd(&mut not_all_true, 131);
    assert_eq!(run_i32(&not_all_true), 0);

    let mut bitmask = Vec::new();
    push_lanes(&mut bitmask, [-1, 0, 1, -32_768, 32_767, 0, 1, -2]);
    push_simd(&mut bitmask, 132); // i16x8.bitmask
    assert_eq!(run_i32(&bitmask), 0b1000_1001);
}

#[test]
fn i16x8_shift_family_masks_scalar_counts_and_preserves_signedness() {
    let mut shl = Vec::new();
    push_splat(&mut shl, 0x4000);
    push_i32_const(&mut shl, 17); // masked to one bit shift
    push_simd(&mut shl, 139); // i16x8.shl
    push_extract_s(&mut shl, 1);
    assert_eq!(run_i32(&shl), -32_768);

    let mut shr_s = Vec::new();
    push_splat(&mut shr_s, -2);
    push_i32_const(&mut shr_s, 1);
    push_simd(&mut shr_s, 140); // i16x8.shr_s
    push_extract_s(&mut shr_s, 5);
    assert_eq!(run_i32(&shr_s), -1);

    let mut shr_u = Vec::new();
    push_splat(&mut shr_u, 0x8000);
    push_i32_const(&mut shr_u, 1);
    push_simd(&mut shr_u, 141); // i16x8.shr_u
    push_extract_u(&mut shr_u, 3);
    assert_eq!(run_i32(&shr_u), 0x4000);
}

#[test]
fn i16x8_min_max_and_unsigned_average_are_lane_independent() {
    let mut min_s = Vec::new();
    push_lanes(&mut min_s, [-1, 7, 300, -400, 5, 6, 7, 8]);
    push_lanes(&mut min_s, [1, 6, -300, -399, 10, 5, 8, 7]);
    push_simd(&mut min_s, 150); // i16x8.min_s
    push_extract_s(&mut min_s, 2);
    assert_eq!(run_i32(&min_s), -300);

    let mut min_u = Vec::new();
    push_splat(&mut min_u, 65_535);
    push_splat(&mut min_u, 1);
    push_simd(&mut min_u, 151); // i16x8.min_u
    push_extract_u(&mut min_u, 0);
    assert_eq!(run_i32(&min_u), 1);

    let mut max_s = Vec::new();
    push_splat(&mut max_s, -1);
    push_splat(&mut max_s, 1);
    push_simd(&mut max_s, 152); // i16x8.max_s
    push_extract_s(&mut max_s, 4);
    assert_eq!(run_i32(&max_s), 1);

    let mut max_u = Vec::new();
    push_splat(&mut max_u, 65_535);
    push_splat(&mut max_u, 1);
    push_simd(&mut max_u, 153); // i16x8.max_u
    push_extract_u(&mut max_u, 6);
    assert_eq!(run_i32(&max_u), 65_535);

    let mut average = Vec::new();
    push_lanes(&mut average, [1, 65_535, 4, 5, 6, 7, 8, 9]);
    push_lanes(&mut average, [2, 65_534, 8, 9, 10, 11, 12, 13]);
    push_simd(&mut average, 155); // i16x8.avgr_u
    push_extract_u(&mut average, 0);
    assert_eq!(run_i32(&average), 2);

    let mut average_hi = Vec::new();
    push_splat(&mut average_hi, 65_535);
    push_splat(&mut average_hi, 65_534);
    push_simd(&mut average_hi, 155);
    push_extract_u(&mut average_hi, 7);
    assert_eq!(run_i32(&average_hi), 65_535);
}

#[test]
fn i16x8_alu_closure_scans_structured_control() {
    let mut instructions = vec![0x02, 0x7f]; // block (result i32)
    push_splat(&mut instructions, -9);
    push_simd(&mut instructions, 128); // i16x8.abs
    push_splat(&mut instructions, 10);
    push_simd(&mut instructions, 152); // i16x8.max_s
    push_extract_s(&mut instructions, 0);
    instructions.push(0x0b); // end block
    assert_eq!(run_i32(&instructions), 10);
}

#[test]
fn validator_rejects_i16x8_alu_type_confusion() {
    let mut binary = Vec::new();
    push_splat(&mut binary, 1);
    push_i32_const(&mut binary, 2); // wrong rhs type
    push_simd(&mut binary, 150); // i16x8.min_s
    push_extract_s(&mut binary, 0);
    let parsed = parse_module(&module(&binary)).expect("binary type-confusion fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));

    let mut shift = Vec::new();
    push_splat(&mut shift, 1);
    push_splat(&mut shift, 2); // wrong shift-count type
    push_simd(&mut shift, 139); // i16x8.shl
    push_extract_s(&mut shift, 0);
    let parsed = parse_module(&module(&shift)).expect("shift type-confusion fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}

#[test]
fn adjacent_f32x4_f64x2_min_frontier_remains_fail_closed() {
    let mut instructions = Vec::new();
    push_splat(&mut instructions, 1);
    push_simd(&mut instructions, 167); // i32x4.extend_low_i16x8_s is supported
    push_splat(&mut instructions, 2);
    push_simd(&mut instructions, 167); // second operand stays V128-typed
    push_simd(&mut instructions, 260); // next f64x2 opcode remains outside this slice
    push_extract_s(&mut instructions, 0);

    let parsed = parse_module(&module(&instructions)).expect("unsupported-SIMD fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 260,
                ..
            }
        ))
    ));
}
