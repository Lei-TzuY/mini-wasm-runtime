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

fn push_f64x2_const(code: &mut Vec<u8>, lanes: [f64; 2]) {
    code.extend_from_slice(&[0xfd, 0x0c]);
    for lane in lanes {
        code.extend_from_slice(&lane.to_bits().to_le_bytes());
    }
}

fn simd(code: &mut Vec<u8>, subopcode: u32) {
    code.push(0xfd);
    push_u32(code, subopcode);
}

fn run_lane(lanes: [f64; 2], lane: u8) -> i32 {
    let mut code = Vec::new();
    push_f64x2_const(&mut code, lanes);
    simd(&mut code, 259);
    code.extend_from_slice(&[0xfd, 0x1b, lane]);
    let parsed = parse_module(&module(&code)).expect("fixture parses");
    let mut instance = Instance::new(parsed).expect("fixture validates");
    match instance
        .invoke_export_values("run", &[])
        .expect("execution succeeds")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn executes_in_range_and_zeroes_upper_lanes() {
    let lanes = [1.75, -12345.75];
    assert_eq!(run_lane(lanes, 0), 1);
    assert_eq!(run_lane(lanes, 1), -12345);
    assert_eq!(run_lane(lanes, 2), 0);
    assert_eq!(run_lane(lanes, 3), 0);
}

#[test]
fn uses_permitted_saturating_choices_for_special_lanes() {
    assert_eq!(run_lane([f64::NAN, f64::INFINITY], 0), 0);
    assert_eq!(run_lane([f64::NAN, f64::INFINITY], 1), i32::MAX);
    assert_eq!(run_lane([f64::NEG_INFINITY, 42.9], 0), i32::MIN);
    assert_eq!(run_lane([f64::NEG_INFINITY, 42.9], 1), 42);
}

#[test]
fn validator_rejects_type_confusion() {
    let mut code = vec![0x41, 0x00];
    simd(&mut code, 259);
    code.extend_from_slice(&[0xfd, 0x1b, 0x00]);
    let parsed = parse_module(&module(&code)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}

#[test]
fn next_subopcode_remains_fail_closed() {
    let mut code = Vec::new();
    push_f64x2_const(&mut code, [1.0; 2]);
    simd(&mut code, 260);
    code.extend_from_slice(&[0xfd, 0x1b, 0x00]);
    let parsed = parse_module(&module(&code)).expect("fixture parses");
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
