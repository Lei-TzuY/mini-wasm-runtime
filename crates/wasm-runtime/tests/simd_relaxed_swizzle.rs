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

fn simd(code: &mut Vec<u8>, subopcode: u32) {
    code.push(0xfd);
    push_u32(code, subopcode);
}

fn run_i32(code: &[u8]) -> i32 {
    let parsed = parse_module(&module(code)).expect("relaxed swizzle fixture must parse");
    let mut instance = Instance::new(parsed).expect("relaxed swizzle fixture must validate");
    match instance
        .invoke_export_values("run", &[])
        .expect("relaxed swizzle must execute")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected relaxed swizzle result: {other:?}"),
    }
}

fn table() -> [u8; 16] {
    [
        10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    ]
}

#[test]
fn relaxed_swizzle_selects_in_range_lanes_and_zeroes_high_indices() {
    let mut code = Vec::new();
    push_v128_const(&mut code, table());
    push_v128_const(
        &mut code,
        [15, 16, 31, 127, 128, 255, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
    );
    simd(&mut code, 256);
    code.extend_from_slice(&[0xfd, 0x16, 0x00]);
    assert_eq!(run_i32(&code), 25);

    let mut high = Vec::new();
    push_v128_const(&mut high, table());
    push_v128_const(
        &mut high,
        [128, 255, 16, 31, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
    );
    simd(&mut high, 256);
    high.extend_from_slice(&[0xfd, 0x16, 0x00]);
    assert_eq!(run_i32(&high), 0);
}

#[test]
fn relaxed_swizzle_executes_inside_structured_control() {
    let mut code = vec![0x02, 0x7f];
    push_v128_const(&mut code, table());
    push_v128_const(
        &mut code,
        [3, 0, 1, 2, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    );
    simd(&mut code, 256);
    code.extend_from_slice(&[0xfd, 0x16, 0x00, 0x0b]);
    assert_eq!(run_i32(&code), 13);
}

#[test]
fn validator_rejects_relaxed_swizzle_type_confusion() {
    let mut code = Vec::new();
    push_v128_const(&mut code, table());
    code.extend_from_slice(&[0x41, 0x00]);
    simd(&mut code, 256);
    code.extend_from_slice(&[0xfd, 0x16, 0x00]);
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
    push_v128_const(&mut code, table());
    push_v128_const(&mut code, table());
    simd(&mut code, 257);
    code.extend_from_slice(&[0xfd, 0x16, 0x00]);
    let parsed = parse_module(&module(&code)).expect("257 frontier fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 257,
                ..
            }
        ))
    ));
}
