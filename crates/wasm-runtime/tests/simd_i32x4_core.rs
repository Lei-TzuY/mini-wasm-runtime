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

#[test]
fn i32x4_arithmetic_executes_inside_structured_control() {
    let instructions = [
        0x02, 0x7f, // block (result i32)
        0x41, 0x0a, // i32.const 10
        0xfd, 0x11, // i32x4.splat
        0xfd, 0x0c, // v128.const i32x4 1 2 3 4
        0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00,
        0x00, 0xfd, 0xae, 0x01, // i32x4.add
        0xfd, 0x1b, 0x02, // i32x4.extract_lane 2
        0x0b, // end block
    ];
    let bytes = module(&instructions);
    let parsed = parse_module(&bytes).expect("SIMD fixture must parse as a core module");
    let mut instance = Instance::new(parsed).expect("initial SIMD i32x4 fixture must validate");
    let result = instance
        .invoke_export_values("run", &[])
        .expect("initial SIMD i32x4 fixture must execute");
    assert_eq!(result, vec![Value::I32(13)]);
}

#[test]
fn validator_rejects_out_of_bounds_i32x4_lane() {
    let bytes = module(&[
        0xfd, 0x0c, // v128.const
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xfd, 0x1b,
        0x04, // i32x4.extract_lane 4
    ]);
    let parsed = parse_module(&bytes).expect("invalid-lane fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::MalformedImmediate { .. }
        ))
    ));
}

#[test]
fn validator_rejects_i32x4_splat_type_confusion() {
    let bytes = module(&[
        0x42, 0x00, // i64.const 0
        0xfd, 0x11, // i32x4.splat requires i32
        0xfd, 0x1b, 0x00, // extract lane to satisfy result if typing were wrong
    ]);
    let parsed = parse_module(&bytes).expect("type-confusion fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}

#[test]
fn unsupported_simd_subopcode_remains_fail_closed() {
    let bytes = module(&[
        0x42, 0x00, // i64.const 0
        0xfd, 0x12, // i64x2.splat is outside this initial slice
        0x41, 0x00, // result if an implementation accidentally skips the opcode
    ]);
    let parsed = parse_module(&bytes).expect("unsupported-SIMD fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 18,
                ..
            }
        ))
    ));
}
