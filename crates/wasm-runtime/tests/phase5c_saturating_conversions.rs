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

fn sat_instructions(subopcode: u32) -> Vec<u8> {
    let mut code = vec![0x20, 0x00, 0xfc];
    push_u32(&mut code, subopcode);
    code
}

fn invoke(param: u8, result: u8, subopcode: u32, value: Value) -> Result<Value, RuntimeError> {
    let bytes = module(param, result, &sat_instructions(subopcode));
    let mut instance = Instance::new(parse_module(&bytes).unwrap()).unwrap();
    instance
        .invoke_export("run", &[value])
        .map(|value| value.unwrap())
}

#[test]
fn nan_saturates_to_zero_for_all_subopcodes() {
    for (subopcode, param, result, value) in [
        (0, F32, I32, Value::F32(f32::NAN)),
        (1, F32, I32, Value::F32(f32::NAN)),
        (2, F64, I32, Value::F64(f64::NAN)),
        (3, F64, I32, Value::F64(f64::NAN)),
        (4, F32, I64, Value::F32(f32::NAN)),
        (5, F32, I64, Value::F32(f32::NAN)),
        (6, F64, I64, Value::F64(f64::NAN)),
        (7, F64, I64, Value::F64(f64::NAN)),
    ] {
        let expected = if result == I32 {
            Value::I32(0)
        } else {
            Value::I64(0)
        };
        assert_eq!(invoke(param, result, subopcode, value).unwrap(), expected);
    }
}

#[test]
fn infinities_clamp_to_signed_and_unsigned_extrema() {
    assert_eq!(
        invoke(F32, I32, 0, Value::F32(f32::NEG_INFINITY)).unwrap(),
        Value::I32(i32::MIN)
    );
    assert_eq!(
        invoke(F32, I32, 0, Value::F32(f32::INFINITY)).unwrap(),
        Value::I32(i32::MAX)
    );
    assert_eq!(
        invoke(F32, I32, 1, Value::F32(f32::NEG_INFINITY)).unwrap(),
        Value::I32(0)
    );
    assert_eq!(
        invoke(F32, I32, 1, Value::F32(f32::INFINITY)).unwrap(),
        Value::I32(-1)
    );
    assert_eq!(
        invoke(F64, I64, 6, Value::F64(f64::NEG_INFINITY)).unwrap(),
        Value::I64(i64::MIN)
    );
    assert_eq!(
        invoke(F64, I64, 6, Value::F64(f64::INFINITY)).unwrap(),
        Value::I64(i64::MAX)
    );
    assert_eq!(
        invoke(F64, I64, 7, Value::F64(f64::NEG_INFINITY)).unwrap(),
        Value::I64(0)
    );
    assert_eq!(
        invoke(F64, I64, 7, Value::F64(f64::INFINITY)).unwrap(),
        Value::I64(-1)
    );
}

#[test]
fn finite_i32_and_u32_overflow_clamps_to_extrema() {
    assert_eq!(
        invoke(F64, I32, 2, Value::F64(-9.0e30)).unwrap(),
        Value::I32(i32::MIN)
    );
    assert_eq!(
        invoke(F64, I32, 2, Value::F64(9.0e30)).unwrap(),
        Value::I32(i32::MAX)
    );
    assert_eq!(
        invoke(F64, I32, 3, Value::F64(-1.0)).unwrap(),
        Value::I32(0)
    );
    assert_eq!(
        invoke(F64, I32, 3, Value::F64(9.0e30)).unwrap(),
        Value::I32(-1)
    );
}

#[test]
fn finite_i64_and_u64_overflow_clamps_to_extrema() {
    assert_eq!(
        invoke(F64, I64, 6, Value::F64(-1.0e300)).unwrap(),
        Value::I64(i64::MIN)
    );
    assert_eq!(
        invoke(F64, I64, 6, Value::F64(1.0e300)).unwrap(),
        Value::I64(i64::MAX)
    );
    assert_eq!(
        invoke(F64, I64, 7, Value::F64(-1.0)).unwrap(),
        Value::I64(0)
    );
    assert_eq!(
        invoke(F64, I64, 7, Value::F64(1.0e300)).unwrap(),
        Value::I64(-1)
    );
}

#[test]
fn finite_in_range_values_truncate_toward_zero() {
    assert_eq!(
        invoke(F32, I32, 0, Value::F32(-42.9)).unwrap(),
        Value::I32(-42)
    );
    assert_eq!(
        invoke(F64, I32, 3, Value::F64(42.9)).unwrap(),
        Value::I32(42)
    );
    assert_eq!(
        invoke(F32, I64, 4, Value::F32(-123_456.75)).unwrap(),
        Value::I64(-123_456)
    );
    assert_eq!(
        invoke(F64, I64, 7, Value::F64(123_456.75)).unwrap(),
        Value::I64(123_456)
    );
}

#[test]
fn unsigned_negative_inputs_always_saturate_to_zero() {
    assert_eq!(
        invoke(F32, I32, 1, Value::F32(-0.75)).unwrap(),
        Value::I32(0)
    );
    assert_eq!(
        invoke(F64, I32, 3, Value::F64(-1.0)).unwrap(),
        Value::I32(0)
    );
    assert_eq!(
        invoke(F32, I64, 5, Value::F32(-12345.0)).unwrap(),
        Value::I64(0)
    );
    assert_eq!(
        invoke(F64, I64, 7, Value::F64(-1.0e300)).unwrap(),
        Value::I64(0)
    );
}

#[test]
fn saturating_conversion_executes_inside_structured_control() {
    let mut instructions = vec![0x02, I32, 0x20, 0x00, 0xfc];
    push_u32(&mut instructions, 2);
    instructions.push(0x0b);
    let bytes = module(F64, I32, &instructions);
    let mut instance = Instance::new(parse_module(&bytes).unwrap()).unwrap();
    assert_eq!(
        instance.invoke_export("run", &[Value::F64(7.9)]).unwrap(),
        Some(Value::I32(7))
    );
}

#[test]
fn validator_rejects_saturating_conversion_type_confusion() {
    let bytes = module(I32, I32, &sat_instructions(0));
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

#[test]
fn unsupported_prefixed_subopcodes_preserve_the_decoded_u32_value() {
    for subopcode in [8u32, 128] {
        let bytes = module(F32, I32, &sat_instructions(subopcode));
        let error = Instance::new(parse_module(&bytes).unwrap()).unwrap_err();
        assert!(matches!(
            error,
            RuntimeError::Validation(ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfc,
                subopcode: actual,
                ..
            }) if actual == subopcode
        ));
    }
}

#[test]
fn malformed_prefixed_subopcode_leb_is_rejected() {
    let bytes = module(F32, I32, &[0x20, 0x00, 0xfc, 0xff, 0xff, 0xff, 0xff, 0x10]);
    let error = Instance::new(parse_module(&bytes).unwrap()).unwrap_err();
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::MalformedImmediate { .. })
    ));
}
