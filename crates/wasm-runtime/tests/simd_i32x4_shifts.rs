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

fn push_i32x4_const(instructions: &mut Vec<u8>, lanes: [i32; 4]) {
    push_simd(instructions, 12);
    for lane in lanes {
        instructions.extend_from_slice(&lane.to_le_bytes());
    }
}

fn push_i32x4_extract(instructions: &mut Vec<u8>, lane: u8) {
    push_simd(instructions, 27);
    instructions.push(lane);
}

fn run_i32(instructions: &[u8]) -> i32 {
    let parsed = parse_module(&module(instructions)).expect("i32x4 shift fixture must parse");
    let mut instance = Instance::new(parsed).expect("i32x4 shift fixture must validate");
    match instance
        .invoke_export_values("run", &[])
        .expect("i32x4 shift fixture must execute")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected i32x4 shift result: {other:?}"),
    }
}

#[test]
fn i32x4_shift_family_masks_counts_and_preserves_signedness() {
    let mut shl = Vec::new();
    push_i32x4_const(&mut shl, [0x4000_0000, 0, 0, 0]);
    push_i32_const(&mut shl, 33);
    push_simd(&mut shl, 171);
    push_i32x4_extract(&mut shl, 0);
    assert_eq!(run_i32(&shl), i32::MIN);

    let mut shr_s = Vec::new();
    push_i32x4_const(&mut shr_s, [-2, 0, 0, 0]);
    push_i32_const(&mut shr_s, 1);
    push_simd(&mut shr_s, 172);
    push_i32x4_extract(&mut shr_s, 0);
    assert_eq!(run_i32(&shr_s), -1);

    let mut shr_u = Vec::new();
    push_i32x4_const(&mut shr_u, [i32::MIN, 0, 0, 0]);
    push_i32_const(&mut shr_u, 1);
    push_simd(&mut shr_u, 173);
    push_i32x4_extract(&mut shr_u, 0);
    assert_eq!(run_i32(&shr_u), 0x4000_0000);
}

#[test]
fn i32x4_shifts_execute_inside_structured_control() {
    let mut instructions = vec![0x02, 0x7f];
    push_i32x4_const(&mut instructions, [1, 0, 0, 0]);
    push_i32_const(&mut instructions, 3);
    push_simd(&mut instructions, 171);
    push_i32x4_extract(&mut instructions, 0);
    instructions.push(0x0b);
    assert_eq!(run_i32(&instructions), 8);
}

#[test]
fn validator_rejects_i32x4_shift_type_confusion() {
    let mut instructions = Vec::new();
    push_i32_const(&mut instructions, 0);
    push_i32_const(&mut instructions, 1);
    push_simd(&mut instructions, 171);
    push_i32x4_extract(&mut instructions, 0);
    let parsed = parse_module(&module(&instructions)).expect("type-confusion fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}

#[test]
fn adjacent_i64x2_comparison_remains_fail_closed() {
    let mut instructions = Vec::new();
    push_i32x4_const(&mut instructions, [1, 0, 0, 0]);
    push_i32x4_const(&mut instructions, [2, 0, 0, 0]);
    push_simd(&mut instructions, 214);
    push_i32x4_extract(&mut instructions, 0);
    let parsed = parse_module(&module(&instructions)).expect("unsupported-SIMD fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 214,
                ..
            }
        ))
    ));
}
