use super::RuntimeError;
use wasm_parser::ValueType;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    FuncRef(Option<u32>),
}

impl Value {
    pub fn value_type(self) -> ValueType {
        match self {
            Self::I32(_) => ValueType::I32,
            Self::I64(_) => ValueType::I64,
            Self::F32(_) => ValueType::F32,
            Self::F64(_) => ValueType::F64,
            Self::FuncRef(_) => ValueType::FuncRef,
        }
    }

    pub fn as_i32(self) -> i32 {
        match self {
            Self::I32(value) => value,
            other => panic!("Value::as_i32 called for {:?}", other.value_type()),
        }
    }

    pub fn as_i64(self) -> i64 {
        match self {
            Self::I64(value) => value,
            other => panic!("Value::as_i64 called for {:?}", other.value_type()),
        }
    }

    pub fn as_f32(self) -> f32 {
        match self {
            Self::F32(value) => value,
            other => panic!("Value::as_f32 called for {:?}", other.value_type()),
        }
    }

    pub fn as_f64(self) -> f64 {
        match self {
            Self::F64(value) => value,
            other => panic!("Value::as_f64 called for {:?}", other.value_type()),
        }
    }
}

pub(super) fn zero(ty: ValueType) -> Value {
    match ty {
        ValueType::I32 => Value::I32(0),
        ValueType::I64 => Value::I64(0),
        ValueType::F32 => Value::F32(0.0),
        ValueType::F64 => Value::F64(0.0),
        ValueType::FuncRef => Value::FuncRef(None),
    }
}

pub(super) fn expect_type(value: Value, expected: ValueType) -> Result<Value, RuntimeError> {
    let actual = value.value_type();
    if actual == expected {
        Ok(value)
    } else {
        Err(RuntimeError::ValueTypeMismatch { expected, actual })
    }
}

pub(super) fn pop_typed(
    stack: &mut Vec<Value>,
    expected: ValueType,
) -> Result<Value, RuntimeError> {
    let value = stack.pop().ok_or(RuntimeError::StackUnderflow)?;
    expect_type(value, expected)
}

pub(super) fn i32_from_stack(stack: &mut Vec<Value>) -> Result<i32, RuntimeError> {
    match pop_typed(stack, ValueType::I32)? {
        Value::I32(value) => Ok(value),
        _ => unreachable!("pop_typed established i32"),
    }
}

pub(super) fn i64_from_stack(stack: &mut Vec<Value>) -> Result<i64, RuntimeError> {
    match pop_typed(stack, ValueType::I64)? {
        Value::I64(value) => Ok(value),
        _ => unreachable!("pop_typed established i64"),
    }
}

pub(super) fn unary_integer(stack: &mut Vec<Value>, opcode: u8) -> Result<(), RuntimeError> {
    let value = match opcode {
        0x67 => Value::I32(i32_from_stack(stack)?.leading_zeros() as i32),
        0x68 => Value::I32(i32_from_stack(stack)?.trailing_zeros() as i32),
        0x69 => Value::I32(i32_from_stack(stack)?.count_ones() as i32),
        0x79 => Value::I64(i64_from_stack(stack)?.leading_zeros() as i64),
        0x7a => Value::I64(i64_from_stack(stack)?.trailing_zeros() as i64),
        0x7b => Value::I64(i64_from_stack(stack)?.count_ones() as i64),
        _ => return Err(RuntimeError::UnsupportedOpcode(opcode)),
    };
    stack.push(value);
    Ok(())
}

pub(super) fn binary_integer(stack: &mut Vec<Value>, opcode: u8) -> Result<(), RuntimeError> {
    let value = match opcode {
        0x6a..=0x78 => {
            let rhs = i32_from_stack(stack)?;
            let lhs = i32_from_stack(stack)?;
            Value::I32(eval_i32(lhs, rhs, opcode)?)
        }
        0x7c..=0x8a => {
            let rhs = i64_from_stack(stack)?;
            let lhs = i64_from_stack(stack)?;
            Value::I64(eval_i64(lhs, rhs, opcode)?)
        }
        _ => return Err(RuntimeError::UnsupportedOpcode(opcode)),
    };
    stack.push(value);
    Ok(())
}

fn eval_i32(lhs: i32, rhs: i32, opcode: u8) -> Result<i32, RuntimeError> {
    Ok(match opcode {
        0x6a => lhs.wrapping_add(rhs),
        0x6b => lhs.wrapping_sub(rhs),
        0x6c => lhs.wrapping_mul(rhs),
        0x6d => {
            if rhs == 0 {
                return Err(RuntimeError::IntegerDivisionByZero);
            }
            if lhs == i32::MIN && rhs == -1 {
                return Err(RuntimeError::IntegerOverflow);
            }
            lhs / rhs
        }
        0x6e => {
            let rhs = rhs as u32;
            if rhs == 0 {
                return Err(RuntimeError::IntegerDivisionByZero);
            }
            ((lhs as u32) / rhs) as i32
        }
        0x6f => {
            if rhs == 0 {
                return Err(RuntimeError::IntegerDivisionByZero);
            }
            if lhs == i32::MIN && rhs == -1 {
                0
            } else {
                lhs % rhs
            }
        }
        0x70 => {
            let rhs = rhs as u32;
            if rhs == 0 {
                return Err(RuntimeError::IntegerDivisionByZero);
            }
            ((lhs as u32) % rhs) as i32
        }
        0x71 => lhs & rhs,
        0x72 => lhs | rhs,
        0x73 => lhs ^ rhs,
        0x74 => lhs.wrapping_shl((rhs as u32) & 31),
        0x75 => lhs.wrapping_shr((rhs as u32) & 31),
        0x76 => ((lhs as u32) >> ((rhs as u32) & 31)) as i32,
        0x77 => lhs.rotate_left((rhs as u32) & 31),
        0x78 => lhs.rotate_right((rhs as u32) & 31),
        _ => return Err(RuntimeError::UnsupportedOpcode(opcode)),
    })
}

fn eval_i64(lhs: i64, rhs: i64, opcode: u8) -> Result<i64, RuntimeError> {
    Ok(match opcode {
        0x7c => lhs.wrapping_add(rhs),
        0x7d => lhs.wrapping_sub(rhs),
        0x7e => lhs.wrapping_mul(rhs),
        0x7f => {
            if rhs == 0 {
                return Err(RuntimeError::IntegerDivisionByZero);
            }
            if lhs == i64::MIN && rhs == -1 {
                return Err(RuntimeError::IntegerOverflow);
            }
            lhs / rhs
        }
        0x80 => {
            let rhs = rhs as u64;
            if rhs == 0 {
                return Err(RuntimeError::IntegerDivisionByZero);
            }
            ((lhs as u64) / rhs) as i64
        }
        0x81 => {
            if rhs == 0 {
                return Err(RuntimeError::IntegerDivisionByZero);
            }
            if lhs == i64::MIN && rhs == -1 {
                0
            } else {
                lhs % rhs
            }
        }
        0x82 => {
            let rhs = rhs as u64;
            if rhs == 0 {
                return Err(RuntimeError::IntegerDivisionByZero);
            }
            ((lhs as u64) % rhs) as i64
        }
        0x83 => lhs & rhs,
        0x84 => lhs | rhs,
        0x85 => lhs ^ rhs,
        0x86 => lhs.wrapping_shl((rhs as u32) & 63),
        0x87 => lhs.wrapping_shr((rhs as u32) & 63),
        0x88 => ((lhs as u64) >> ((rhs as u32) & 63)) as i64,
        0x89 => lhs.rotate_left((rhs as u32) & 63),
        0x8a => lhs.rotate_right((rhs as u32) & 63),
        _ => return Err(RuntimeError::UnsupportedOpcode(opcode)),
    })
}

const F32_SIGN: u32 = 0x8000_0000;
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
    if lhs <= rhs {
        lhs
    } else {
        rhs
    }
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
    if lhs >= rhs {
        lhs
    } else {
        rhs
    }
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
    if lhs <= rhs {
        lhs
    } else {
        rhs
    }
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
    if lhs >= rhs {
        lhs
    } else {
        rhs
    }
}

pub(super) fn compare_i32(stack: &mut Vec<Value>, opcode: u8) -> Result<(), RuntimeError> {
    if opcode == 0x45 {
        let value = i32_from_stack(stack)?;
        stack.push(Value::I32(i32::from(value == 0)));
        return Ok(());
    }
    let rhs = i32_from_stack(stack)?;
    let lhs = i32_from_stack(stack)?;
    let result = match opcode {
        0x46 => lhs == rhs,
        0x47 => lhs != rhs,
        0x48 => lhs < rhs,
        0x49 => (lhs as u32) < (rhs as u32),
        0x4a => lhs > rhs,
        0x4b => (lhs as u32) > (rhs as u32),
        0x4c => lhs <= rhs,
        0x4d => (lhs as u32) <= (rhs as u32),
        0x4e => lhs >= rhs,
        0x4f => (lhs as u32) >= (rhs as u32),
        _ => return Err(RuntimeError::UnsupportedOpcode(opcode)),
    };
    stack.push(Value::I32(i32::from(result)));
    Ok(())
}

pub(super) fn compare_i64(stack: &mut Vec<Value>, opcode: u8) -> Result<(), RuntimeError> {
    if opcode == 0x50 {
        let value = match pop_typed(stack, ValueType::I64)? {
            Value::I64(value) => value,
            _ => unreachable!("pop_typed established i64"),
        };
        stack.push(Value::I32(i32::from(value == 0)));
        return Ok(());
    }
    let rhs = match pop_typed(stack, ValueType::I64)? {
        Value::I64(value) => value,
        _ => unreachable!("pop_typed established i64"),
    };
    let lhs = match pop_typed(stack, ValueType::I64)? {
        Value::I64(value) => value,
        _ => unreachable!("pop_typed established i64"),
    };
    let result = match opcode {
        0x51 => lhs == rhs,
        0x52 => lhs != rhs,
        0x53 => lhs < rhs,
        0x54 => (lhs as u64) < (rhs as u64),
        0x55 => lhs > rhs,
        0x56 => (lhs as u64) > (rhs as u64),
        0x57 => lhs <= rhs,
        0x58 => (lhs as u64) <= (rhs as u64),
        0x59 => lhs >= rhs,
        0x5a => (lhs as u64) >= (rhs as u64),
        _ => return Err(RuntimeError::UnsupportedOpcode(opcode)),
    };
    stack.push(Value::I32(i32::from(result)));
    Ok(())
}

pub(super) fn compare_f32(stack: &mut Vec<Value>, opcode: u8) -> Result<(), RuntimeError> {
    let rhs = match pop_typed(stack, ValueType::F32)? {
        Value::F32(value) => value,
        _ => unreachable!("pop_typed established f32"),
    };
    let lhs = match pop_typed(stack, ValueType::F32)? {
        Value::F32(value) => value,
        _ => unreachable!("pop_typed established f32"),
    };
    let result = match opcode {
        0x5b => lhs == rhs,
        0x5c => lhs != rhs,
        0x5d => lhs < rhs,
        0x5e => lhs > rhs,
        0x5f => lhs <= rhs,
        0x60 => lhs >= rhs,
        _ => return Err(RuntimeError::UnsupportedOpcode(opcode)),
    };
    stack.push(Value::I32(i32::from(result)));
    Ok(())
}

pub(super) fn compare_f64(stack: &mut Vec<Value>, opcode: u8) -> Result<(), RuntimeError> {
    let rhs = match pop_typed(stack, ValueType::F64)? {
        Value::F64(value) => value,
        _ => unreachable!("pop_typed established f64"),
    };
    let lhs = match pop_typed(stack, ValueType::F64)? {
        Value::F64(value) => value,
        _ => unreachable!("pop_typed established f64"),
    };
    let result = match opcode {
        0x61 => lhs == rhs,
        0x62 => lhs != rhs,
        0x63 => lhs < rhs,
        0x64 => lhs > rhs,
        0x65 => lhs <= rhs,
        0x66 => lhs >= rhs,
        _ => return Err(RuntimeError::UnsupportedOpcode(opcode)),
    };
    stack.push(Value::I32(i32::from(result)));
    Ok(())
}

pub(super) fn convert(stack: &mut Vec<Value>, opcode: u8) -> Result<(), RuntimeError> {
    let value = match opcode {
        0xa7 => Value::I32(i64_from_stack(stack)? as i32),
        0xa8 => Value::I32(trunc_to_i32(f64::from(f32_from_stack(stack)?), true)?),
        0xa9 => Value::I32(trunc_to_i32(f64::from(f32_from_stack(stack)?), false)?),
        0xaa => Value::I32(trunc_to_i32(f64_from_stack(stack)?, true)?),
        0xab => Value::I32(trunc_to_i32(f64_from_stack(stack)?, false)?),
        0xac => Value::I64(i64::from(i32_from_stack(stack)?)),
        0xad => Value::I64(i64::from(i32_from_stack(stack)? as u32)),
        0xae => Value::I64(trunc_to_i64(f64::from(f32_from_stack(stack)?), true)?),
        0xaf => Value::I64(trunc_to_i64(f64::from(f32_from_stack(stack)?), false)?),
        0xb0 => Value::I64(trunc_to_i64(f64_from_stack(stack)?, true)?),
        0xb1 => Value::I64(trunc_to_i64(f64_from_stack(stack)?, false)?),
        0xb2 => Value::F32(i32_from_stack(stack)? as f32),
        0xb3 => Value::F32((i32_from_stack(stack)? as u32) as f32),
        0xb4 => Value::F32(i64_from_stack(stack)? as f32),
        0xb5 => Value::F32((i64_from_stack(stack)? as u64) as f32),
        0xb6 => Value::F32(f64_from_stack(stack)? as f32),
        0xb7 => Value::F64(f64::from(i32_from_stack(stack)?)),
        0xb8 => Value::F64(f64::from(i32_from_stack(stack)? as u32)),
        0xb9 => Value::F64(i64_from_stack(stack)? as f64),
        0xba => Value::F64((i64_from_stack(stack)? as u64) as f64),
        0xbb => Value::F64(f64::from(f32_from_stack(stack)?)),
        0xbc => Value::I32(f32_from_stack(stack)?.to_bits() as i32),
        0xbd => Value::I64(f64_from_stack(stack)?.to_bits() as i64),
        0xbe => Value::F32(f32::from_bits(i32_from_stack(stack)? as u32)),
        0xbf => Value::F64(f64::from_bits(i64_from_stack(stack)? as u64)),
        _ => return Err(RuntimeError::UnsupportedOpcode(opcode)),
    };
    stack.push(value);
    Ok(())
}

fn trunc_to_i32(value: f64, signed: bool) -> Result<i32, RuntimeError> {
    if value.is_nan() {
        return Err(RuntimeError::InvalidConversionToInteger);
    }
    if !value.is_finite() {
        return Err(RuntimeError::IntegerOverflow);
    }
    let value = value.trunc();
    if signed {
        const LOWER: f64 = -2_147_483_648.0;
        const UPPER: f64 = 2_147_483_648.0;
        if !(LOWER..UPPER).contains(&value) {
            return Err(RuntimeError::IntegerOverflow);
        }
        Ok(value as i32)
    } else {
        const UPPER: f64 = 4_294_967_296.0;
        if !(0.0..UPPER).contains(&value) {
            return Err(RuntimeError::IntegerOverflow);
        }
        Ok((value as u32) as i32)
    }
}

fn trunc_to_i64(value: f64, signed: bool) -> Result<i64, RuntimeError> {
    if value.is_nan() {
        return Err(RuntimeError::InvalidConversionToInteger);
    }
    if !value.is_finite() {
        return Err(RuntimeError::IntegerOverflow);
    }
    let value = value.trunc();
    if signed {
        const LOWER: f64 = -9_223_372_036_854_775_808.0;
        const UPPER: f64 = 9_223_372_036_854_775_808.0;
        if !(LOWER..UPPER).contains(&value) {
            return Err(RuntimeError::IntegerOverflow);
        }
        Ok(value as i64)
    } else {
        const UPPER: f64 = 18_446_744_073_709_551_616.0;
        if !(0.0..UPPER).contains(&value) {
            return Err(RuntimeError::IntegerOverflow);
        }
        Ok((value as u64) as i64)
    }
}

pub(super) fn trunc_sat(stack: &mut Vec<Value>, subopcode: u32) -> Result<(), RuntimeError> {
    let value = match subopcode {
        0 => Value::I32(trunc_sat_i32(f64::from(f32_from_stack(stack)?), true)),
        1 => Value::I32(trunc_sat_i32(f64::from(f32_from_stack(stack)?), false)),
        2 => Value::I32(trunc_sat_i32(f64_from_stack(stack)?, true)),
        3 => Value::I32(trunc_sat_i32(f64_from_stack(stack)?, false)),
        4 => Value::I64(trunc_sat_i64(f64::from(f32_from_stack(stack)?), true)),
        5 => Value::I64(trunc_sat_i64(f64::from(f32_from_stack(stack)?), false)),
        6 => Value::I64(trunc_sat_i64(f64_from_stack(stack)?, true)),
        7 => Value::I64(trunc_sat_i64(f64_from_stack(stack)?, false)),
        _ => {
            return Err(RuntimeError::UnsupportedPrefixedOpcode {
                prefix: 0xfc,
                subopcode,
            })
        }
    };
    stack.push(value);
    Ok(())
}

fn trunc_sat_i32(value: f64, signed: bool) -> i32 {
    if value.is_nan() {
        return 0;
    }
    let value = value.trunc();
    if signed {
        const LOWER: f64 = -2_147_483_648.0;
        const UPPER: f64 = 2_147_483_648.0;
        if value <= LOWER {
            i32::MIN
        } else if value >= UPPER {
            i32::MAX
        } else {
            value as i32
        }
    } else {
        const UPPER: f64 = 4_294_967_296.0;
        if value <= 0.0 {
            0
        } else if value >= UPPER {
            -1
        } else {
            (value as u32) as i32
        }
    }
}

fn trunc_sat_i64(value: f64, signed: bool) -> i64 {
    if value.is_nan() {
        return 0;
    }
    let value = value.trunc();
    if signed {
        const LOWER: f64 = -9_223_372_036_854_775_808.0;
        const UPPER: f64 = 9_223_372_036_854_775_808.0;
        if value <= LOWER {
            i64::MIN
        } else if value >= UPPER {
            i64::MAX
        } else {
            value as i64
        }
    } else {
        const UPPER: f64 = 18_446_744_073_709_551_616.0;
        if value <= 0.0 {
            0
        } else if value >= UPPER {
            -1
        } else {
            (value as u64) as i64
        }
    }
}
