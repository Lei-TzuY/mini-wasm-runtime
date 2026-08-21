use super::RuntimeError;
use wasm_parser::ValueType;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
}

impl Value {
    pub fn value_type(self) -> ValueType {
        match self {
            Self::I32(_) => ValueType::I32,
            Self::I64(_) => ValueType::I64,
            Self::F32(_) => ValueType::F32,
            Self::F64(_) => ValueType::F64,
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

pub(super) fn binary_f32(
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
        0xa7 => {
            let value = match pop_typed(stack, ValueType::I64)? {
                Value::I64(value) => value,
                _ => unreachable!("pop_typed established i64"),
            };
            Value::I32(value as i32)
        }
        0xac => Value::I64(i64::from(i32_from_stack(stack)?)),
        0xad => Value::I64(i64::from(i32_from_stack(stack)? as u32)),
        0xb6 => {
            let value = match pop_typed(stack, ValueType::F64)? {
                Value::F64(value) => value,
                _ => unreachable!("pop_typed established f64"),
            };
            Value::F32(value as f32)
        }
        0xbb => {
            let value = match pop_typed(stack, ValueType::F32)? {
                Value::F32(value) => value,
                _ => unreachable!("pop_typed established f32"),
            };
            Value::F64(f64::from(value))
        }
        _ => return Err(RuntimeError::UnsupportedOpcode(opcode)),
    };
    stack.push(value);
    Ok(())
}
