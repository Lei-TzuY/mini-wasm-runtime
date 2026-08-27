use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};

const I32: u8 = 0x7f;
const F32: u8 = 0x7d;
const F64: u8 = 0x7c;

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

fn single_result_module(result: u8, instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x01, result]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);

    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn f32_const(value: f32) -> Vec<u8> {
    let mut bytes = vec![0x43];
    bytes.extend(value.to_bits().to_le_bytes());
    bytes
}

fn f64_const(value: f64) -> Vec<u8> {
    let mut bytes = vec![0x44];
    bytes.extend(value.to_bits().to_le_bytes());
    bytes
}

fn invoke(module: &[u8]) -> Value {
    let module = parse_module(module).expect("positive conformance fixture must parse");
    let mut instance = Instance::new(module).expect("positive conformance fixture must validate");
    instance
        .invoke_export("run", &[])
        .expect("positive conformance fixture must execute")
        .expect("positive conformance fixture must return one value")
}

#[test]
fn positive_and_negative_zero_compare_equal() {
    let mut f32_instructions = f32_const(0.0);
    f32_instructions.extend(f32_const(-0.0));
    f32_instructions.push(0x5b); // f32.eq
    assert_eq!(
        invoke(&single_result_module(I32, &f32_instructions)),
        Value::I32(1)
    );

    let mut f64_instructions = f64_const(0.0);
    f64_instructions.extend(f64_const(-0.0));
    f64_instructions.push(0x61); // f64.eq
    assert_eq!(
        invoke(&single_result_module(I32, &f64_instructions)),
        Value::I32(1)
    );
}

#[test]
fn nan_ordered_comparisons_are_false() {
    for opcode in 0x5d..=0x60 {
        let mut instructions = f32_const(f32::NAN);
        instructions.extend(f32_const(1.0));
        instructions.push(opcode); // f32.lt/gt/le/ge
        assert_eq!(
            invoke(&single_result_module(I32, &instructions)),
            Value::I32(0),
            "f32 comparison opcode 0x{opcode:02x}"
        );
    }

    for opcode in 0x63..=0x66 {
        let mut instructions = f64_const(f64::NAN);
        instructions.extend(f64_const(1.0));
        instructions.push(opcode); // f64.lt/gt/le/ge
        assert_eq!(
            invoke(&single_result_module(I32, &instructions)),
            Value::I32(0),
            "f64 comparison opcode 0x{opcode:02x}"
        );
    }
}

#[test]
fn division_by_signed_zero_produces_signed_infinity() {
    for (denominator, expected) in [(0.0f32, f32::INFINITY), (-0.0, f32::NEG_INFINITY)] {
        let mut instructions = f32_const(1.0);
        instructions.extend(f32_const(denominator));
        instructions.push(0x95); // f32.div
        assert_eq!(
            invoke(&single_result_module(F32, &instructions)),
            Value::F32(expected)
        );
    }

    for (denominator, expected) in [(0.0f64, f64::INFINITY), (-0.0, f64::NEG_INFINITY)] {
        let mut instructions = f64_const(1.0);
        instructions.extend(f64_const(denominator));
        instructions.push(0xa3); // f64.div
        assert_eq!(
            invoke(&single_result_module(F64, &instructions)),
            Value::F64(expected)
        );
    }
}

#[test]
fn zero_divided_by_zero_produces_nan() {
    let mut f32_instructions = f32_const(0.0);
    f32_instructions.extend(f32_const(0.0));
    f32_instructions.push(0x95); // f32.div
    match invoke(&single_result_module(F32, &f32_instructions)) {
        Value::F32(value) => assert!(value.is_nan()),
        other => panic!("expected f32 result, got {other:?}"),
    }

    let mut f64_instructions = f64_const(0.0);
    f64_instructions.extend(f64_const(0.0));
    f64_instructions.push(0xa3); // f64.div
    match invoke(&single_result_module(F64, &f64_instructions)) {
        Value::F64(value) => assert!(value.is_nan()),
        other => panic!("expected f64 result, got {other:?}"),
    }
}

#[test]
fn invalid_float_arithmetic_operations_produce_nan_without_payload_assumptions() {
    for (opcode, lhs, rhs) in [
        (0x92, f32::INFINITY, f32::NEG_INFINITY), // add
        (0x93, f32::INFINITY, f32::INFINITY),     // sub
        (0x94, 0.0, f32::INFINITY),               // mul
        (0x95, f32::INFINITY, f32::INFINITY),     // div
    ] {
        let mut instructions = f32_const(lhs);
        instructions.extend(f32_const(rhs));
        instructions.push(opcode);
        match invoke(&single_result_module(F32, &instructions)) {
            Value::F32(value) => assert!(
                value.is_nan(),
                "f32 opcode 0x{opcode:02x} must produce NaN for invalid operands"
            ),
            other => panic!("expected f32 result, got {other:?}"),
        }
    }

    for (opcode, lhs, rhs) in [
        (0xa0, f64::INFINITY, f64::NEG_INFINITY), // add
        (0xa1, f64::INFINITY, f64::INFINITY),     // sub
        (0xa2, 0.0, f64::INFINITY),               // mul
        (0xa3, f64::INFINITY, f64::INFINITY),     // div
    ] {
        let mut instructions = f64_const(lhs);
        instructions.extend(f64_const(rhs));
        instructions.push(opcode);
        match invoke(&single_result_module(F64, &instructions)) {
            Value::F64(value) => assert!(
                value.is_nan(),
                "f64 opcode 0x{opcode:02x} must produce NaN for invalid operands"
            ),
            other => panic!("expected f64 result, got {other:?}"),
        }
    }
}

#[test]
fn promote_and_demote_preserve_negative_zero() {
    let mut promote = f32_const(-0.0);
    promote.push(0xbb); // f64.promote_f32
    match invoke(&single_result_module(F64, &promote)) {
        Value::F64(value) => assert_eq!(value.to_bits(), (-0.0f64).to_bits()),
        other => panic!("expected f64 result, got {other:?}"),
    }

    let mut demote = f64_const(-0.0);
    demote.push(0xb6); // f32.demote_f64
    match invoke(&single_result_module(F32, &demote)) {
        Value::F32(value) => assert_eq!(value.to_bits(), (-0.0f32).to_bits()),
        other => panic!("expected f32 result, got {other:?}"),
    }
}
