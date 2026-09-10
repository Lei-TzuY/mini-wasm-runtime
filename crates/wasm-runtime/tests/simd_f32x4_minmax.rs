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
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x00, 0x01, 0x7f]);
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
fn push_f32x4_const(i: &mut Vec<u8>, lanes: [f32; 4]) {
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
fn lane_bits(lhs: [f32; 4], rhs: [f32; 4], subopcode: u32, lane: u32) -> u32 {
    let mut i = Vec::new();
    push_i32_const(&mut i, 0);
    push_f32x4_const(&mut i, lhs);
    push_f32x4_const(&mut i, rhs);
    push_simd(&mut i, subopcode);
    push_simd(&mut i, 11);
    i.extend_from_slice(&[4, 0]);
    push_i32_const(&mut i, 0);
    i.push(0x28);
    i.push(2);
    push_u32(&mut i, lane * 4);
    let parsed = parse_module(&module(&i)).expect("fixture parses");
    let mut instance = Instance::new(parsed).expect("fixture validates");
    match instance
        .invoke_export_values("run", &[])
        .expect("executes")
        .as_slice()
    {
        [Value::I32(v)] => *v as u32,
        other => panic!("unexpected result: {other:?}"),
    }
}
#[test]
fn f32x4_min_max_cover_order_and_signed_zero() {
    assert_eq!(
        lane_bits([3.0, -2.0, 8.0, 1.0], [4.0, -5.0, 7.0, 2.0], 232, 0),
        3.0f32.to_bits()
    );
    assert_eq!(
        lane_bits([3.0, -2.0, 8.0, 1.0], [4.0, -5.0, 7.0, 2.0], 233, 2),
        8.0f32.to_bits()
    );
    assert_eq!(lane_bits([0.0; 4], [-0.0; 4], 232, 0), (-0.0f32).to_bits());
    assert_eq!(lane_bits([-0.0; 4], [0.0; 4], 233, 0), 0.0f32.to_bits());
}
#[test]
fn f32x4_min_max_propagate_nan() {
    let min = f32::from_bits(lane_bits([f32::NAN; 4], [1.0; 4], 232, 0));
    let max = f32::from_bits(lane_bits([1.0; 4], [f32::NAN; 4], 233, 0));
    assert!(min.is_nan());
    assert!(max.is_nan());
}
#[test]
fn validator_rejects_f32x4_min_type_confusion() {
    let mut i = Vec::new();
    push_f32x4_const(&mut i, [1.0; 4]);
    push_i32_const(&mut i, 1);
    push_simd(&mut i, 232);
    let parsed = parse_module(&module(&i)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}
#[test]
fn adjacent_f32x4_f64x2_abs_frontier_remains_fail_closed() {
    let mut i = Vec::new();
    push_f32x4_const(&mut i, [1.0; 4]);
    push_f32x4_const(&mut i, [2.0; 4]);
    push_simd(&mut i, 236);
    let parsed = parse_module(&module(&i)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 236,
                ..
            }
        ))
    ));
}

#[test]
fn f32x4_pmin_pmax_preserve_lhs_on_unordered_or_equal_inputs() {
    assert_eq!(lane_bits([0.0; 4], [-0.0; 4], 234, 0), 0.0f32.to_bits());
    assert_eq!(lane_bits([-0.0; 4], [0.0; 4], 235, 0), (-0.0f32).to_bits());
    let lhs_nan = f32::from_bits(lane_bits([f32::NAN; 4], [1.0; 4], 234, 0));
    assert!(lhs_nan.is_nan());
    assert_eq!(lane_bits([1.0; 4], [f32::NAN; 4], 234, 0), 1.0f32.to_bits());
    assert_eq!(lane_bits([1.0; 4], [f32::NAN; 4], 235, 0), 1.0f32.to_bits());
}

#[test]
fn f32x4_pmin_pmax_cover_ordered_lanes() {
    assert_eq!(
        lane_bits([3.0, -2.0, 8.0, 1.0], [4.0, -5.0, 7.0, 2.0], 234, 1),
        (-5.0f32).to_bits()
    );
    assert_eq!(
        lane_bits([3.0, -2.0, 8.0, 1.0], [4.0, -5.0, 7.0, 2.0], 235, 3),
        2.0f32.to_bits()
    );
}

#[test]
fn validator_rejects_f32x4_pmin_type_confusion() {
    let mut i = Vec::new();
    push_f32x4_const(&mut i, [1.0; 4]);
    push_i32_const(&mut i, 1);
    push_simd(&mut i, 234);
    let parsed = parse_module(&module(&i)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}
