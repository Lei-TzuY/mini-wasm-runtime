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

fn run_i32(instructions: &[u8]) -> i32 {
    let parsed = parse_module(&module(instructions)).expect("i8x16 lane fixture must parse");
    let mut instance = Instance::new(parsed).expect("i8x16 lane fixture must validate");
    match instance
        .invoke_export_values("run", &[])
        .expect("i8x16 lane fixture must execute")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected i8x16 lane result: {other:?}"),
    }
}

#[test]
fn i8x16_splat_extracts_signed_and_unsigned_lanes() {
    // i32.const -128; i8x16.splat; i8x16.extract_lane_s 7
    assert_eq!(
        run_i32(&[0x41, 0x80, 0x7f, 0xfd, 0x0f, 0xfd, 0x15, 0x07]),
        -128
    );

    // The same stored byte is zero-extended by extract_lane_u.
    assert_eq!(
        run_i32(&[0x41, 0x80, 0x7f, 0xfd, 0x0f, 0xfd, 0x16, 0x0c]),
        128
    );
}

#[test]
fn i8x16_replace_lane_truncates_and_preserves_other_lanes() {
    // Replace lane 15 with -1: the lane stores 0xff and extracts as 255 unsigned.
    assert_eq!(
        run_i32(&[
            0x41, 0x07, // i32.const 7
            0xfd, 0x0f, // i8x16.splat
            0x41, 0x7f, // i32.const -1
            0xfd, 0x17, 0x0f, // i8x16.replace_lane 15
            0xfd, 0x16, 0x0f, // i8x16.extract_lane_u 15
        ]),
        255
    );

    // A different lane remains untouched.
    assert_eq!(
        run_i32(&[
            0x41, 0x07, // i32.const 7
            0xfd, 0x0f, // i8x16.splat
            0x41, 0x7f, // i32.const -1
            0xfd, 0x17, 0x0f, // i8x16.replace_lane 15
            0xfd, 0x16, 0x00, // i8x16.extract_lane_u 0
        ]),
        7
    );
}

#[test]
fn i8x16_lane_ops_execute_inside_structured_control() {
    assert_eq!(
        run_i32(&[
            0x02, 0x7f, // block (result i32)
            0x41, 0x80, 0x7f, // i32.const -128
            0xfd, 0x0f, // i8x16.splat
            0xfd, 0x15, 0x05, // i8x16.extract_lane_s 5
            0x0b, // end block
        ]),
        -128
    );
}

#[test]
fn validator_rejects_out_of_bounds_i8x16_lane() {
    let bytes = module(&[
        0x41, 0x00, // i32.const 0
        0xfd, 0x0f, // i8x16.splat
        0xfd, 0x16, 0x10, // i8x16.extract_lane_u 16
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
fn validator_rejects_i8x16_splat_type_confusion() {
    let bytes = module(&[
        0x42, 0x00, // i64.const 0
        0xfd, 0x0f, // i8x16.splat requires i32
        0xfd, 0x16, 0x00, // extract lane if typing were wrong
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
fn validator_rejects_i8x16_replace_lane_type_confusion() {
    let bytes = module(&[
        0x41, 0x00, // i32.const 0
        0xfd, 0x0f, // i8x16.splat
        0x42, 0x00, // i64.const 0 where replace_lane requires i32
        0xfd, 0x17, 0x03, // i8x16.replace_lane 3
        0xfd, 0x16, 0x03, // extract result if typing were wrong
    ]);
    let parsed = parse_module(&bytes).expect("replace-lane type-confusion fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}
