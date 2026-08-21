from pathlib import Path


def replace_once(path, old, new, label):
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one anchor, found {count}")
    p.write_text(text.replace(old, new, 1))


runtime = "crates/wasm-runtime/src/lib.rs"
numeric = "crates/wasm-runtime/src/numeric.rs"
validator = "crates/wasm-validator/src/typed.rs"

replace_once(
    runtime,
    '''                0x92 => numeric::binary_f32(&mut stack, |a, b| a + b)?,
                0x93 => numeric::binary_f32(&mut stack, |a, b| a - b)?,
                0x94 => numeric::binary_f32(&mut stack, |a, b| a * b)?,
                0x95 => numeric::binary_f32(&mut stack, |a, b| a / b)?,
                0xa0 => numeric::binary_f64(&mut stack, |a, b| a + b)?,
                0xa1 => numeric::binary_f64(&mut stack, |a, b| a - b)?,
                0xa2 => numeric::binary_f64(&mut stack, |a, b| a * b)?,
                0xa3 => numeric::binary_f64(&mut stack, |a, b| a / b)?,
''',
    '''                0x8b..=0x91 | 0x99..=0x9f => numeric::unary_float(&mut stack, opcode)?,
                0x92..=0x98 | 0xa0..=0xa6 => numeric::binary_float(&mut stack, opcode)?,
''',
    "runtime float dispatch",
)

replace_once(
    runtime,
    '''            | 0x92..=0x95
            | 0xa0..=0xa3
            | 0xa7
''',
    '''            | 0x8b..=0xa6
            | 0xa7
''',
    "control map float range",
)

old_helpers = '''pub(super) fn binary_f32(
    stack: &mut Vec<Value>,
    operation: fn(f32, f32) -> f32,
) -> Result<(), RuntimeError> {
    let rhs = match pop_typed(stack, ValueType::F32)? {
        Value::F32(value) => value,
        _ => unreachable!("pop_typed established f32"),
    };
    let lhs = match pop_typed(stack, ValueType::F32)? {
        Value::F32(value) => value,
        _ => unreachable!("pop_typed established f32"),
    };
    stack.push(Value::F32(operation(lhs, rhs)));
    Ok(())
}

pub(super) fn binary_f64(
    stack: &mut Vec<Value>,
    operation: fn(f64, f64) -> f64,
) -> Result<(), RuntimeError> {
    let rhs = match pop_typed(stack, ValueType::F64)? {
        Value::F64(value) => value,
        _ => unreachable!("pop_typed established f64"),
    };
    let lhs = match pop_typed(stack, ValueType::F64)? {
        Value::F64(value) => value,
        _ => unreachable!("pop_typed established f64"),
    };
    stack.push(Value::F64(operation(lhs, rhs)));
    Ok(())
}

'''
new_helpers = r'''const F32_SIGN: u32 = 0x8000_0000;
const F64_SIGN: u64 = 0x8000_0000_0000_0000;
const F32_CANONICAL_NAN: u32 = 0x7fc0_0000;
const F64_CANONICAL_NAN: u64 = 0x7ff8_0000_0000_0000;

fn f32_from_stack(stack: &mut Vec<Value>) -> Result<f32, RuntimeError> {
    match pop_typed(stack, ValueType::F32)? {
        Value::F32(value) => Ok(value),
        _ => unreachable!("pop_typed established f32"),
    }
}

fn f64_from_stack(stack: &mut Vec<Value>) -> Result<f64, RuntimeError> {
    match pop_typed(stack, ValueType::F64)? {
        Value::F64(value) => Ok(value),
        _ => unreachable!("pop_typed established f64"),
    }
}

pub(super) fn unary_float(stack: &mut Vec<Value>, opcode: u8) -> Result<(), RuntimeError> {
    let value = match opcode {
        0x8b..=0x91 => Value::F32(eval_unary_f32(f32_from_stack(stack)?, opcode)?),
        0x99..=0x9f => Value::F64(eval_unary_f64(f64_from_stack(stack)?, opcode)?),
        _ => return Err(RuntimeError::UnsupportedOpcode(opcode)),
    };
    stack.push(value);
    Ok(())
}

pub(super) fn binary_float(stack: &mut Vec<Value>, opcode: u8) -> Result<(), RuntimeError> {
    let value = match opcode {
        0x92..=0x98 => {
            let rhs = f32_from_stack(stack)?;
            let lhs = f32_from_stack(stack)?;
            Value::F32(eval_binary_f32(lhs, rhs, opcode)?)
        }
        0xa0..=0xa6 => {
            let rhs = f64_from_stack(stack)?;
            let lhs = f64_from_stack(stack)?;
            Value::F64(eval_binary_f64(lhs, rhs, opcode)?)
        }
        _ => return Err(RuntimeError::UnsupportedOpcode(opcode)),
    };
    stack.push(value);
    Ok(())
}

fn eval_unary_f32(value: f32, opcode: u8) -> Result<f32, RuntimeError> {
    Ok(match opcode {
        0x8b => f32::from_bits(value.to_bits() & !F32_SIGN),
        0x8c => f32::from_bits(value.to_bits() ^ F32_SIGN),
        0x8d => directed_f32(value, f32::ceil),
        0x8e => directed_f32(value, f32::floor),
        0x8f => directed_f32(value, f32::trunc),
        0x90 => nearest_f32(value),
        0x91 => sqrt_f32(value),
        _ => return Err(RuntimeError::UnsupportedOpcode(opcode)),
    })
}

fn eval_unary_f64(value: f64, opcode: u8) -> Result<f64, RuntimeError> {
    Ok(match opcode {
        0x99 => f64::from_bits(value.to_bits() & !F64_SIGN),
        0x9a => f64::from_bits(value.to_bits() ^ F64_SIGN),
        0x9b => directed_f64(value, f64::ceil),
        0x9c => directed_f64(value, f64::floor),
        0x9d => directed_f64(value, f64::trunc),
        0x9e => nearest_f64(value),
        0x9f => sqrt_f64(value),
        _ => return Err(RuntimeError::UnsupportedOpcode(opcode)),
    })
}

fn eval_binary_f32(lhs: f32, rhs: f32, opcode: u8) -> Result<f32, RuntimeError> {
    Ok(match opcode {
        0x92 => lhs + rhs,
        0x93 => lhs - rhs,
        0x94 => lhs * rhs,
        0x95 => lhs / rhs,
        0x96 => min_f32(lhs, rhs),
        0x97 => max_f32(lhs, rhs),
        0x98 => f32::from_bits((lhs.to_bits() & !F32_SIGN) | (rhs.to_bits() & F32_SIGN)),
        _ => return Err(RuntimeError::UnsupportedOpcode(opcode)),
    })
}

fn eval_binary_f64(lhs: f64, rhs: f64, opcode: u8) -> Result<f64, RuntimeError> {
    Ok(match opcode {
        0xa0 => lhs + rhs,
        0xa1 => lhs - rhs,
        0xa2 => lhs * rhs,
        0xa3 => lhs / rhs,
        0xa4 => min_f64(lhs, rhs),
        0xa5 => max_f64(lhs, rhs),
        0xa6 => f64::from_bits((lhs.to_bits() & !F64_SIGN) | (rhs.to_bits() & F64_SIGN)),
        _ => return Err(RuntimeError::UnsupportedOpcode(opcode)),
    })
}

fn canonical_nan_f32() -> f32 {
    f32::from_bits(F32_CANONICAL_NAN)
}

fn canonical_nan_f64() -> f64 {
    f64::from_bits(F64_CANONICAL_NAN)
}

fn directed_f32(value: f32, operation: fn(f32) -> f32) -> f32 {
    if value.is_nan() {
        return canonical_nan_f32();
    }
    if value.is_infinite() || value == 0.0 {
        return value;
    }
    let result = operation(value);
    if result == 0.0 && value.is_sign_negative() {
        f32::from_bits(F32_SIGN)
    } else {
        result
    }
}

fn directed_f64(value: f64, operation: fn(f64) -> f64) -> f64 {
    if value.is_nan() {
        return canonical_nan_f64();
    }
    if value.is_infinite() || value == 0.0 {
        return value;
    }
    let result = operation(value);
    if result == 0.0 && value.is_sign_negative() {
        f64::from_bits(F64_SIGN)
    } else {
        result
    }
}

fn nearest_f32(value: f32) -> f32 {
    if value.is_nan() {
        return canonical_nan_f32();
    }
    if value.is_infinite() || value == 0.0 || value.abs() >= 8_388_608.0 {
        return value;
    }
    let lower = value.floor();
    let fraction = value - lower;
    let result = if fraction < 0.5 {
        lower
    } else if fraction > 0.5 {
        lower + 1.0
    } else if (lower as i32) & 1 == 0 {
        lower
    } else {
        lower + 1.0
    };
    if result == 0.0 && value.is_sign_negative() {
        f32::from_bits(F32_SIGN)
    } else {
        result
    }
}

fn nearest_f64(value: f64) -> f64 {
    if value.is_nan() {
        return canonical_nan_f64();
    }
    if value.is_infinite() || value == 0.0 || value.abs() >= 4_503_599_627_370_496.0 {
        return value;
    }
    let lower = value.floor();
    let fraction = value - lower;
    let result = if fraction < 0.5 {
        lower
    } else if fraction > 0.5 {
        lower + 1.0
    } else if (lower as i64) & 1 == 0 {
        lower
    } else {
        lower + 1.0
    };
    if result == 0.0 && value.is_sign_negative() {
        f64::from_bits(F64_SIGN)
    } else {
        result
    }
}

fn sqrt_f32(value: f32) -> f32 {
    if value.is_nan() || value < 0.0 {
        return canonical_nan_f32();
    }
    if value == 0.0 || value.is_infinite() {
        return value;
    }
    value.sqrt()
}

fn sqrt_f64(value: f64) -> f64 {
    if value.is_nan() || value < 0.0 {
        return canonical_nan_f64();
    }
    if value == 0.0 || value.is_infinite() {
        return value;
    }
    value.sqrt()
}

fn min_f32(lhs: f32, rhs: f32) -> f32 {
    if lhs.is_nan() || rhs.is_nan() {
        return canonical_nan_f32();
    }
    if lhs == 0.0 && rhs == 0.0 {
        return if lhs.is_sign_negative() || rhs.is_sign_negative() {
            f32::from_bits(F32_SIGN)
        } else {
            0.0
        };
    }
    if lhs <= rhs { lhs } else { rhs }
}

fn max_f32(lhs: f32, rhs: f32) -> f32 {
    if lhs.is_nan() || rhs.is_nan() {
        return canonical_nan_f32();
    }
    if lhs == 0.0 && rhs == 0.0 {
        return if lhs.is_sign_positive() || rhs.is_sign_positive() {
            0.0
        } else {
            f32::from_bits(F32_SIGN)
        };
    }
    if lhs >= rhs { lhs } else { rhs }
}

fn min_f64(lhs: f64, rhs: f64) -> f64 {
    if lhs.is_nan() || rhs.is_nan() {
        return canonical_nan_f64();
    }
    if lhs == 0.0 && rhs == 0.0 {
        return if lhs.is_sign_negative() || rhs.is_sign_negative() {
            f64::from_bits(F64_SIGN)
        } else {
            0.0
        };
    }
    if lhs <= rhs { lhs } else { rhs }
}

fn max_f64(lhs: f64, rhs: f64) -> f64 {
    if lhs.is_nan() || rhs.is_nan() {
        return canonical_nan_f64();
    }
    if lhs == 0.0 && rhs == 0.0 {
        return if lhs.is_sign_positive() || rhs.is_sign_positive() {
            0.0
        } else {
            f64::from_bits(F64_SIGN)
        };
    }
    if lhs >= rhs { lhs } else { rhs }
}

'''
replace_once(numeric, old_helpers, new_helpers, "numeric float helpers")

replace_once(
    validator,
    '''            0x92..=0x95 => {
                binary_same(&mut stack, &controls, ValueType::F32, function, offset)?;
            }
            0xa0..=0xa3 => {
                binary_same(&mut stack, &controls, ValueType::F64, function, offset)?;
            }
''',
    '''            0x8b..=0x91 => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::F32,
                    ValueType::F32,
                    function,
                    offset,
                )?;
            }
            0x92..=0x98 => {
                binary_same(&mut stack, &controls, ValueType::F32, function, offset)?;
            }
            0x99..=0x9f => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::F64,
                    ValueType::F64,
                    function,
                    offset,
                )?;
            }
            0xa0..=0xa6 => {
                binary_same(&mut stack, &controls, ValueType::F64, function, offset)?;
            }
''',
    "validator float ranges",
)

Path("crates/wasm-runtime/tests/phase5c_float_operators.rs").write_text(r'''use wasm_parser::{parse_module, ValueType};
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::ValidationError;

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

fn module(params: &[u8], result: u8, instructions: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let mut types = vec![0x01, 0x60];
    push_u32(&mut types, params.len() as u32);
    types.extend_from_slice(params);
    types.extend([0x01, result]);
    push_section(&mut bytes, 1, &types);
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

fn instance(bytes: &[u8]) -> Instance {
    Instance::new(parse_module(bytes).expect("parse float operator fixture"))
        .expect("validate float operator fixture")
}

fn unary_f32(opcode: u8, value: f32) -> f32 {
    let bytes = module(&[F32], F32, &[0x20, 0x00, opcode]);
    instance(&bytes)
        .invoke_export("run", &[Value::F32(value)])
        .unwrap()
        .unwrap()
        .as_f32()
}

fn unary_f64(opcode: u8, value: f64) -> f64 {
    let bytes = module(&[F64], F64, &[0x20, 0x00, opcode]);
    instance(&bytes)
        .invoke_export("run", &[Value::F64(value)])
        .unwrap()
        .unwrap()
        .as_f64()
}

fn binary_f32(opcode: u8, lhs: f32, rhs: f32) -> f32 {
    let bytes = module(&[F32, F32], F32, &[0x20, 0x00, 0x20, 0x01, opcode]);
    instance(&bytes)
        .invoke_export("run", &[Value::F32(lhs), Value::F32(rhs)])
        .unwrap()
        .unwrap()
        .as_f32()
}

fn binary_f64(opcode: u8, lhs: f64, rhs: f64) -> f64 {
    let bytes = module(&[F64, F64], F64, &[0x20, 0x00, 0x20, 0x01, opcode]);
    instance(&bytes)
        .invoke_export("run", &[Value::F64(lhs), Value::F64(rhs)])
        .unwrap()
        .unwrap()
        .as_f64()
}

#[test]
fn abs_and_neg_are_exact_sign_bit_operations() {
    let f32_nan_bits = 0xffc1_2345u32;
    assert_eq!(
        unary_f32(0x8b, f32::from_bits(f32_nan_bits)).to_bits(),
        f32_nan_bits & 0x7fff_ffff
    );
    assert_eq!(
        unary_f32(0x8c, f32::from_bits(f32_nan_bits)).to_bits(),
        f32_nan_bits ^ 0x8000_0000
    );
    assert_eq!(unary_f32(0x8b, -0.0).to_bits(), 0x0000_0000);
    assert_eq!(unary_f32(0x8c, 0.0).to_bits(), 0x8000_0000);

    let f64_nan_bits = 0xfff8_0000_dead_beefu64;
    assert_eq!(
        unary_f64(0x99, f64::from_bits(f64_nan_bits)).to_bits(),
        f64_nan_bits & 0x7fff_ffff_ffff_ffff
    );
    assert_eq!(
        unary_f64(0x9a, f64::from_bits(f64_nan_bits)).to_bits(),
        f64_nan_bits ^ 0x8000_0000_0000_0000
    );
}

#[test]
fn ceil_floor_and_trunc_preserve_directed_rounding_and_signed_zero() {
    assert_eq!(unary_f32(0x8d, 1.25), 2.0);
    assert_eq!(unary_f32(0x8e, 1.75), 1.0);
    assert_eq!(unary_f32(0x8f, -1.75), -1.0);
    assert_eq!(unary_f32(0x8d, -0.25).to_bits(), 0x8000_0000);
    assert_eq!(unary_f32(0x8f, -0.25).to_bits(), 0x8000_0000);

    assert_eq!(unary_f64(0x9b, 1.25), 2.0);
    assert_eq!(unary_f64(0x9c, 1.75), 1.0);
    assert_eq!(unary_f64(0x9d, -1.75), -1.0);
    assert_eq!(unary_f64(0x9b, -0.25).to_bits(), 0x8000_0000_0000_0000);
}

#[test]
fn nearest_uses_ties_to_even_and_preserves_negative_zero() {
    for (value, expected) in [(2.5, 2.0), (3.5, 4.0), (-2.5, -2.0), (-3.5, -4.0)] {
        assert_eq!(unary_f32(0x90, value), expected);
    }
    assert_eq!(unary_f32(0x90, -0.5).to_bits(), 0x8000_0000);
    assert_eq!(unary_f32(0x90, -0.25).to_bits(), 0x8000_0000);
    assert_eq!(unary_f32(0x90, 8_388_608.0), 8_388_608.0);

    for (value, expected) in [(2.5, 2.0), (3.5, 4.0), (-2.5, -2.0), (-3.5, -4.0)] {
        assert_eq!(unary_f64(0x9e, value), expected);
    }
    assert_eq!(
        unary_f64(0x9e, -0.5).to_bits(),
        0x8000_0000_0000_0000
    );
    assert_eq!(
        unary_f64(0x9e, 4_503_599_627_370_496.0),
        4_503_599_627_370_496.0
    );
}

#[test]
fn sqrt_handles_normal_values_signed_zero_and_invalid_inputs() {
    assert_eq!(unary_f32(0x91, 9.0), 3.0);
    assert_eq!(unary_f64(0x9f, 16.0), 4.0);
    assert_eq!(unary_f32(0x91, -0.0).to_bits(), 0x8000_0000);
    assert_eq!(
        unary_f64(0x9f, -0.0).to_bits(),
        0x8000_0000_0000_0000
    );
    assert!(unary_f32(0x91, -1.0).is_nan());
    assert!(unary_f64(0x9f, f64::NEG_INFINITY).is_nan());
}

#[test]
fn min_and_max_obey_signed_zero_rules() {
    assert_eq!(binary_f32(0x96, 0.0, -0.0).to_bits(), 0x8000_0000);
    assert_eq!(binary_f32(0x97, 0.0, -0.0).to_bits(), 0x0000_0000);
    assert_eq!(binary_f32(0x96, -0.0, -0.0).to_bits(), 0x8000_0000);
    assert_eq!(binary_f32(0x97, -0.0, -0.0).to_bits(), 0x8000_0000);

    assert_eq!(
        binary_f64(0xa4, 0.0, -0.0).to_bits(),
        0x8000_0000_0000_0000
    );
    assert_eq!(binary_f64(0xa5, 0.0, -0.0).to_bits(), 0x0000_0000_0000_0000);
}

#[test]
fn min_max_and_rounding_ops_produce_valid_nan_results() {
    let f32_nan = f32::from_bits(0x7fc1_2345);
    assert_eq!(binary_f32(0x96, f32_nan, 1.0).to_bits(), 0x7fc0_0000);
    assert_eq!(binary_f32(0x97, 1.0, f32_nan).to_bits(), 0x7fc0_0000);
    assert_eq!(unary_f32(0x90, f32_nan).to_bits(), 0x7fc0_0000);

    let f64_nan = f64::from_bits(0x7ff8_0000_dead_beef);
    assert_eq!(binary_f64(0xa4, f64_nan, 1.0).to_bits(), 0x7ff8_0000_0000_0000);
    assert_eq!(binary_f64(0xa5, 1.0, f64_nan).to_bits(), 0x7ff8_0000_0000_0000);
    assert_eq!(unary_f64(0x9e, f64_nan).to_bits(), 0x7ff8_0000_0000_0000);
}

#[test]
fn copysign_splices_only_the_sign_bit() {
    let f32_bits = 0x7fc1_2345u32;
    assert_eq!(
        binary_f32(0x98, f32::from_bits(f32_bits), -1.0).to_bits(),
        f32_bits | 0x8000_0000
    );
    assert_eq!(
        binary_f32(0x98, f32::from_bits(f32_bits | 0x8000_0000), 1.0).to_bits(),
        f32_bits
    );

    let f64_bits = 0x7ff8_0000_dead_beefu64;
    assert_eq!(
        binary_f64(0xa6, f64::from_bits(f64_bits), -0.0).to_bits(),
        f64_bits | 0x8000_0000_0000_0000
    );
}

#[test]
fn existing_float_arithmetic_remains_on_the_unified_dispatch() {
    assert_eq!(binary_f32(0x92, 1.5, 2.25), 3.75);
    assert_eq!(binary_f32(0x95, 9.0, 4.5), 2.0);
    assert_eq!(binary_f64(0xa2, 1.5, 4.0), 6.0);
    assert_eq!(binary_f64(0xa3, 9.0, 4.5), 2.0);
}

#[test]
fn float_operators_execute_inside_structured_control() {
    let bytes = module(&[F32], F32, &[0x02, F32, 0x20, 0x00, 0x8b, 0x0b]);
    let result = instance(&bytes)
        .invoke_export("run", &[Value::F32(-3.5)])
        .unwrap()
        .unwrap();
    assert_eq!(result, Value::F32(3.5));
}

#[test]
fn validator_rejects_float_operator_type_confusion() {
    let bytes = module(&[F64], F64, &[0x20, 0x00, 0x8b]);
    let error = Instance::new(parse_module(&bytes).unwrap()).unwrap_err();
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::TypeMismatch {
            expected: ValueType::F32,
            actual: ValueType::F64,
            ..
        })
    ));
}
''')
