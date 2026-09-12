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

fn push_f32x4_const(code: &mut Vec<u8>, lanes: [f32; 4]) {
    code.extend_from_slice(&[0xfd, 0x0c]);
    for lane in lanes {
        code.extend_from_slice(&lane.to_bits().to_le_bytes());
    }
}

fn simd(code: &mut Vec<u8>, subopcode: u32) {
    code.push(0xfd);
    push_u32(code, subopcode);
}

fn run_lane(lanes: [f32; 4], lane: u8) -> i32 {
    let mut code = Vec::new();
    push_f32x4_const(&mut code, lanes);
    simd(&mut code, 257);
    code.extend_from_slice(&[0xfd, 0x1b, lane]);
    let parsed = parse_module(&module(&code)).expect("relaxed trunc fixture must parse");
    let mut instance = Instance::new(parsed).expect("relaxed trunc fixture must validate");
    match instance
        .invoke_export_values("run", &[])
        .expect("relaxed trunc must execute")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected relaxed trunc result: {other:?}"),
    }
}

#[test]
fn relaxed_trunc_executes_deterministic_in_range_lanes() {
    let lanes = [1.75, -2.75, 0.0, 12345.5];
    assert_eq!(run_lane(lanes, 0), 1);
    assert_eq!(run_lane(lanes, 1), -2);
    assert_eq!(run_lane(lanes, 2), 0);
    assert_eq!(run_lane(lanes, 3), 12345);
}

#[test]
fn relaxed_trunc_uses_permitted_saturating_choices_for_special_lanes() {
    let lanes = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 42.9];
    assert_eq!(run_lane(lanes, 0), 0);
    assert_eq!(run_lane(lanes, 1), i32::MAX);
    assert_eq!(run_lane(lanes, 2), i32::MIN);
    assert_eq!(run_lane(lanes, 3), 42);
}

#[test]
fn validator_rejects_relaxed_trunc_type_confusion() {
    let mut code = vec![0x41, 0x00];
    simd(&mut code, 257);
    code.extend_from_slice(&[0xfd, 0x1b, 0x00]);
    let parsed = parse_module(&module(&code)).expect("type-confusion fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}

#[test]
fn next_relaxed_simd_subopcode_remains_fail_closed() {
    let mut code = Vec::new();
    push_f32x4_const(&mut code, [1.0; 4]);
    simd(&mut code, 260);
    code.extend_from_slice(&[0xfd, 0x1b, 0x00]);
    let parsed = parse_module(&module(&code)).expect("259 frontier fixture must parse");
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
