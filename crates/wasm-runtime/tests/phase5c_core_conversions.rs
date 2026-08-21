use wasm_parser::{parse_module, ValueType};
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::ValidationError;

const I32: u8 = 0x7f;
const I64: u8 = 0x7e;
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

fn module(param: u8, result: u8, instructions: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x01, param, 0x01, result]);
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

fn invoke(param: u8, result: u8, opcode: u8, value: Value) -> Result<Value, RuntimeError> {
    let bytes = module(param, result, &[0x20, 0x00, opcode]);
    let mut instance = Instance::new(parse_module(&bytes).unwrap()).unwrap();
    instance
        .invoke_export("run", &[value])
        .map(|value| value.unwrap())
}

#[test]
fn signed_truncations_round_toward_zero_for_both_widths() {
    assert_eq!(
        invoke(F32, I32, 0xa8, Value::F32(42.9)).unwrap(),
        Value::I32(42)
    );
    assert_eq!(
        invoke(F64, I32, 0xaa, Value::F64(-42.9)).unwrap(),
        Value::I32(-42)
    );
    assert_eq!(
        invoke(F32, I64, 0xae, Value::F32(123_456.75)).unwrap(),
        Value::I64(123_456)
    );
    assert_eq!(
        invoke(F64, I64, 0xb0, Value::F64(-123_456.75)).unwrap(),
        Value::I64(-123_456)
    );
}

#[test]
fn unsigned_truncations_accept_negative_fractions_that_truncate_to_zero() {
    assert_eq!(
        invoke(F32, I32, 0xa9, Value::F32(-0.75)).unwrap(),
        Value::I32(0)
    );
    assert_eq!(
        invoke(F64, I32, 0xab, Value::F64(-0.999)).unwrap(),
        Value::I32(0)
    );
    assert_eq!(
        invoke(F32, I64, 0xaf, Value::F32(-0.5)).unwrap(),
        Value::I64(0)
    );
    assert_eq!(
        invoke(F64, I64, 0xb1, Value::F64(-0.999_999)).unwrap(),
        Value::I64(0)
    );
}

#[test]
fn nan_truncations_report_invalid_conversion() {
    for (opcode, param, result, value) in [
        (0xa8, F32, I32, Value::F32(f32::NAN)),
        (0xa9, F32, I32, Value::F32(f32::NAN)),
        (0xaa, F64, I32, Value::F64(f64::NAN)),
        (0xab, F64, I32, Value::F64(f64::NAN)),
        (0xae, F32, I64, Value::F32(f32::NAN)),
        (0xaf, F32, I64, Value::F32(f32::NAN)),
        (0xb0, F64, I64, Value::F64(f64::NAN)),
        (0xb1, F64, I64, Value::F64(f64::NAN)),
    ] {
        assert!(matches!(
            invoke(param, result, opcode, value),
            Err(RuntimeError::InvalidConversionToInteger)
        ));
    }
}

#[test]
fn infinities_and_out_of_range_values_report_integer_overflow() {
    for (opcode, param, result, value) in [
        (0xa8, F32, I32, Value::F32(f32::INFINITY)),
        (0xa9, F32, I32, Value::F32(f32::NEG_INFINITY)),
        (0xaa, F64, I32, Value::F64(f64::INFINITY)),
        (0xab, F64, I32, Value::F64(f64::NEG_INFINITY)),
        (0xae, F32, I64, Value::F32(f32::INFINITY)),
        (0xaf, F32, I64, Value::F32(f32::NEG_INFINITY)),
        (0xb0, F64, I64, Value::F64(f64::INFINITY)),
        (0xb1, F64, I64, Value::F64(f64::NEG_INFINITY)),
    ] {
        assert!(matches!(
            invoke(param, result, opcode, value),
            Err(RuntimeError::IntegerOverflow)
        ));
    }
}

#[test]
fn i32_truncation_boundaries_are_exact() {
    assert_eq!(
        invoke(F64, I32, 0xaa, Value::F64(-2_147_483_648.0)).unwrap(),
        Value::I32(i32::MIN)
    );
    assert_eq!(
        invoke(F64, I32, 0xab, Value::F64(4_294_967_295.0)).unwrap(),
        Value::I32(-1)
    );
    assert!(matches!(
        invoke(F64, I32, 0xaa, Value::F64(2_147_483_648.0)),
        Err(RuntimeError::IntegerOverflow)
    ));
    assert!(matches!(
        invoke(F64, I32, 0xab, Value::F64(4_294_967_296.0)),
        Err(RuntimeError::IntegerOverflow)
    ));
    assert!(matches!(
        invoke(F64, I32, 0xab, Value::F64(-1.0)),
        Err(RuntimeError::IntegerOverflow)
    ));

    assert_eq!(
        invoke(F32, I32, 0xa8, Value::F32(2_147_483_520.0)).unwrap(),
        Value::I32(2_147_483_520)
    );
    assert!(matches!(
        invoke(F32, I32, 0xa8, Value::F32(2_147_483_648.0)),
        Err(RuntimeError::IntegerOverflow)
    ));
    assert_eq!(
        invoke(F32, I32, 0xa9, Value::F32(4_294_967_040.0)).unwrap(),
        Value::I32(-256)
    );
    assert!(matches!(
        invoke(F32, I32, 0xa9, Value::F32(4_294_967_296.0)),
        Err(RuntimeError::IntegerOverflow)
    ));
}

#[test]
fn i64_truncation_boundaries_are_exact() {
    assert_eq!(
        invoke(F64, I64, 0xb0, Value::F64(-9_223_372_036_854_775_808.0)).unwrap(),
        Value::I64(i64::MIN)
    );
    assert_eq!(
        invoke(F64, I64, 0xb0, Value::F64(9_223_372_036_854_774_784.0)).unwrap(),
        Value::I64(9_223_372_036_854_774_784)
    );
    assert!(matches!(
        invoke(F64, I64, 0xb0, Value::F64(9_223_372_036_854_775_808.0)),
        Err(RuntimeError::IntegerOverflow)
    ));

    assert_eq!(
        invoke(F64, I64, 0xb1, Value::F64(18_446_744_073_709_549_568.0)).unwrap(),
        Value::I64(-2048)
    );
    assert!(matches!(
        invoke(F64, I64, 0xb1, Value::F64(18_446_744_073_709_551_616.0)),
        Err(RuntimeError::IntegerOverflow)
    ));
    assert!(matches!(
        invoke(F64, I64, 0xb1, Value::F64(-1.0)),
        Err(RuntimeError::IntegerOverflow)
    ));
}

#[test]
fn integer_to_float_conversions_preserve_signed_and_unsigned_views() {
    assert_eq!(
        invoke(I32, F32, 0xb2, Value::I32(-123)).unwrap(),
        Value::F32(-123.0)
    );
    assert_eq!(
        invoke(I32, F32, 0xb3, Value::I32(-1)).unwrap(),
        Value::F32(u32::MAX as f32)
    );
    assert_eq!(
        invoke(I64, F32, 0xb4, Value::I64(-123_456_789)).unwrap(),
        Value::F32(-123_456_789i64 as f32)
    );
    assert_eq!(
        invoke(I64, F32, 0xb5, Value::I64(-1)).unwrap(),
        Value::F32(u64::MAX as f32)
    );

    assert_eq!(
        invoke(I32, F64, 0xb7, Value::I32(-123)).unwrap(),
        Value::F64(-123.0)
    );
    assert_eq!(
        invoke(I32, F64, 0xb8, Value::I32(-1)).unwrap(),
        Value::F64(u32::MAX as f64)
    );
    assert_eq!(
        invoke(I64, F64, 0xb9, Value::I64(-123_456_789)).unwrap(),
        Value::F64(-123_456_789.0)
    );
    assert_eq!(
        invoke(I64, F64, 0xba, Value::I64(-1)).unwrap(),
        Value::F64(u64::MAX as f64)
    );
}

#[test]
fn integer_to_float_rounding_uses_ties_to_even() {
    assert_eq!(
        invoke(I32, F32, 0xb2, Value::I32(16_777_217)).unwrap(),
        Value::F32(16_777_216.0)
    );
    assert_eq!(
        invoke(I32, F32, 0xb2, Value::I32(-16_777_217)).unwrap(),
        Value::F32(-16_777_216.0)
    );
    assert_eq!(
        invoke(I64, F64, 0xb9, Value::I64(9_007_199_254_740_993)).unwrap(),
        Value::F64(9_007_199_254_740_992.0)
    );
}

#[test]
fn conversions_execute_inside_structured_control() {
    let bytes = module(F64, I32, &[0x02, I32, 0x20, 0x00, 0xaa, 0x0b]);
    let mut instance = Instance::new(parse_module(&bytes).unwrap()).unwrap();
    assert_eq!(
        instance.invoke_export("run", &[Value::F64(-7.9)]).unwrap(),
        Some(Value::I32(-7))
    );
}

#[test]
fn validator_rejects_conversion_type_confusion() {
    let bytes = module(I32, I32, &[0x20, 0x00, 0xa8]);
    let error = Instance::new(parse_module(&bytes).unwrap()).unwrap_err();
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::TypeMismatch {
            expected: ValueType::F32,
            actual: ValueType::I32,
            ..
        })
    ));
}
