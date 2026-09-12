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
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0, 0, 0];
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x00, 0x01, 0x7e]);
    push_section(&mut bytes, 3, &[0x01, 0x00]);
    push_section(&mut bytes, 5, &[0x01, 0x00, 0x01]);
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

fn push_simd(i: &mut Vec<u8>, subopcode: u32) {
    i.push(0xfd);
    push_u32(i, subopcode);
}

fn push_f64x2_const(i: &mut Vec<u8>, lanes: [f64; 2]) {
    push_simd(i, 12);
    for lane in lanes {
        i.extend_from_slice(&lane.to_bits().to_le_bytes());
    }
}

fn push_i32_const(i: &mut Vec<u8>, value: i32) {
    i.push(0x41);
    let mut value = value;
    loop {
        let byte = (value as u8) & 0x7f;
        value >>= 7;
        let sign = byte & 0x40 != 0;
        let done = (value == 0 && !sign) || (value == -1 && sign);
        i.push(if done { byte } else { byte | 0x80 });
        if done {
            break;
        }
    }
}

fn push_v128_store(i: &mut Vec<u8>) {
    push_simd(i, 11);
    i.extend_from_slice(&[4, 0]);
}

fn push_i64_load(i: &mut Vec<u8>, offset: u32) {
    i.push(0x29);
    i.push(3);
    push_u32(i, offset);
}

fn lane_bits(lhs: [f64; 2], rhs: [f64; 2], subopcode: u32, lane: u32) -> u64 {
    let mut instructions = Vec::new();
    instructions.extend_from_slice(&[0x02, 0x40]);
    push_f64x2_const(&mut instructions, lhs);
    push_f64x2_const(&mut instructions, rhs);
    push_simd(&mut instructions, subopcode);
    instructions.push(0x1a);
    instructions.push(0x0b);
    push_i32_const(&mut instructions, 0);
    push_f64x2_const(&mut instructions, lhs);
    push_f64x2_const(&mut instructions, rhs);
    push_simd(&mut instructions, subopcode);
    push_v128_store(&mut instructions);
    push_i32_const(&mut instructions, 0);
    push_i64_load(&mut instructions, lane * 8);

    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    let mut instance = Instance::new(parsed).expect("fixture validates");
    match instance
        .invoke_export_values("run", &[])
        .expect("fixture executes")
        .as_slice()
    {
        [Value::I64(value)] => *value as u64,
        other => panic!("unexpected f64x2 minmax result: {other:?}"),
    }
}

#[test]
fn f64x2_min_max_are_lane_exact_and_preserve_signed_zero() {
    assert_eq!(lane_bits([3.0, 0.0], [2.0, -0.0], 244, 0), 2.0f64.to_bits());
    assert_eq!(
        lane_bits([3.0, 0.0], [2.0, -0.0], 244, 1),
        (-0.0f64).to_bits()
    );
    assert_eq!(lane_bits([3.0, 0.0], [2.0, -0.0], 245, 0), 3.0f64.to_bits());
    assert_eq!(lane_bits([3.0, 0.0], [2.0, -0.0], 245, 1), 0.0f64.to_bits());
}

#[test]
fn f64x2_min_max_propagate_nan() {
    for op in [244, 245] {
        assert!(f64::from_bits(lane_bits([f64::NAN, 1.0], [2.0, f64::NAN], op, 0)).is_nan());
        assert!(f64::from_bits(lane_bits([f64::NAN, 1.0], [2.0, f64::NAN], op, 1)).is_nan());
    }
}

#[test]
fn validator_rejects_f64x2_minmax_type_confusion() {
    let mut instructions = Vec::new();
    push_f64x2_const(&mut instructions, [1.0; 2]);
    push_i32_const(&mut instructions, 1);
    push_simd(&mut instructions, 244);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}

#[test]
fn adjacent_f64x2_conversion_frontier_remains_fail_closed() {
    let mut instructions = Vec::new();
    push_f64x2_const(&mut instructions, [1.0, 2.0]);
    push_f64x2_const(&mut instructions, [3.0, 4.0]);
    push_simd(&mut instructions, 259);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 259,
                ..
            }
        ))
    ));
}

#[test]
fn f64x2_pmin_pmax_preserve_lhs_on_unordered_or_equal_inputs() {
    assert_eq!(lane_bits([0.0; 2], [-0.0; 2], 246, 0), 0.0f64.to_bits());
    assert_eq!(lane_bits([-0.0; 2], [0.0; 2], 247, 0), (-0.0f64).to_bits());

    let lhs_nan = f64::from_bits(lane_bits([f64::NAN; 2], [1.0; 2], 246, 0));
    assert!(lhs_nan.is_nan());
    assert_eq!(lane_bits([1.0; 2], [f64::NAN; 2], 246, 0), 1.0f64.to_bits());
    assert_eq!(lane_bits([1.0; 2], [f64::NAN; 2], 247, 0), 1.0f64.to_bits());
}

#[test]
fn f64x2_pmin_pmax_cover_ordered_lanes() {
    assert_eq!(
        lane_bits([3.0, -2.0], [4.0, -5.0], 246, 1),
        (-5.0f64).to_bits()
    );
    assert_eq!(
        lane_bits([3.0, -2.0], [4.0, -5.0], 247, 0),
        4.0f64.to_bits()
    );
}

#[test]
fn validator_rejects_f64x2_pmin_type_confusion() {
    let mut instructions = Vec::new();
    push_f64x2_const(&mut instructions, [1.0; 2]);
    push_i32_const(&mut instructions, 1);
    push_simd(&mut instructions, 246);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}
