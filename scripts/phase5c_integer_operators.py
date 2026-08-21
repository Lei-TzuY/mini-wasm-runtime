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
    '''    UnsupportedOpcode(u8),
    UnsupportedBlockType(u8),
''',
    '''    UnsupportedOpcode(u8),
    IntegerDivisionByZero,
    IntegerOverflow,
    UnsupportedBlockType(u8),
''',
    "runtime integer trap variants",
)

replace_once(
    runtime,
    '''            Self::UnsupportedOpcode(opcode) => write!(f, "unsupported opcode 0x{opcode:02x}"),
            Self::UnsupportedBlockType(block_type) => {
''',
    '''            Self::UnsupportedOpcode(opcode) => write!(f, "unsupported opcode 0x{opcode:02x}"),
            Self::IntegerDivisionByZero => write!(f, "integer division by zero"),
            Self::IntegerOverflow => write!(f, "integer signed division overflow"),
            Self::UnsupportedBlockType(block_type) => {
''',
    "runtime integer trap display",
)

replace_once(
    runtime,
    '''                0x6a => numeric::binary_i32(&mut stack, i32::wrapping_add)?,
                0x6b => numeric::binary_i32(&mut stack, i32::wrapping_sub)?,
                0x6c => numeric::binary_i32(&mut stack, i32::wrapping_mul)?,
                0x7c => numeric::binary_i64(&mut stack, i64::wrapping_add)?,
                0x7d => numeric::binary_i64(&mut stack, i64::wrapping_sub)?,
                0x7e => numeric::binary_i64(&mut stack, i64::wrapping_mul)?,
''',
    '''                0x67..=0x69 => numeric::unary_integer(&mut stack, opcode)?,
                0x6a..=0x78 => numeric::binary_integer(&mut stack, opcode)?,
                0x79..=0x7b => numeric::unary_integer(&mut stack, opcode)?,
                0x7c..=0x8a => numeric::binary_integer(&mut stack, opcode)?,
''',
    "runtime integer dispatch",
)

replace_once(
    runtime,
    '''            | 0x6a..=0x6c
            | 0x7c..=0x7e
            | 0x92..=0x95
''',
    '''            | 0x67..=0x8a
            | 0x92..=0x95
''',
    "control map integer opcode range",
)

insert_anchor = '''pub(super) fn binary_f32(
'''
helpers = r'''pub(super) fn i64_from_stack(stack: &mut Vec<Value>) -> Result<i64, RuntimeError> {
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

'''
p = Path(numeric)
text = p.read_text()
if text.count(insert_anchor) != 1:
    raise SystemExit("numeric insertion anchor mismatch")
p.write_text(text.replace(insert_anchor, helpers + insert_anchor, 1))

replace_once(
    validator,
    '''            0x6a..=0x6c => binary_same(&mut stack, &controls, ValueType::I32, function, offset)?,
            0x7c..=0x7e => {
                binary_same(&mut stack, &controls, ValueType::I64, function, offset)?;
            }
''',
    '''            0x67..=0x69 => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::I32,
                    ValueType::I32,
                    function,
                    offset,
                )?;
            }
            0x6a..=0x78 => binary_same(&mut stack, &controls, ValueType::I32, function, offset)?,
            0x79..=0x7b => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::I64,
                    ValueType::I64,
                    function,
                    offset,
                )?;
            }
            0x7c..=0x8a => {
                binary_same(&mut stack, &controls, ValueType::I64, function, offset)?;
            }
''',
    "validator integer ranges",
)

Path("crates/wasm-runtime/tests/phase5c_integer_operators.rs").write_text(r'''use wasm_parser::{parse_module, ValueType};
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::ValidationError;

const I32: u8 = 0x7f;
const I64: u8 = 0x7e;

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
    Instance::new(parse_module(bytes).expect("parse integer operator fixture"))
        .expect("validate integer operator fixture")
}

fn binary_i32(opcode: u8, lhs: i32, rhs: i32) -> Result<Option<Value>, RuntimeError> {
    let bytes = module(&[I32, I32], I32, &[0x20, 0x00, 0x20, 0x01, opcode]);
    instance(&bytes).invoke_export("run", &[Value::I32(lhs), Value::I32(rhs)])
}

fn binary_i64(opcode: u8, lhs: i64, rhs: i64) -> Result<Option<Value>, RuntimeError> {
    let bytes = module(&[I64, I64], I64, &[0x20, 0x00, 0x20, 0x01, opcode]);
    instance(&bytes).invoke_export("run", &[Value::I64(lhs), Value::I64(rhs)])
}

#[test]
fn count_operators_cover_i32_and_i64() {
    for (opcode, value, expected) in [(0x67, 0i32, 32), (0x68, 8, 3), (0x69, 0xf0f0, 8)] {
        let bytes = module(&[I32], I32, &[0x20, 0x00, opcode]);
        assert_eq!(
            instance(&bytes).invoke_export("run", &[Value::I32(value)]).unwrap(),
            Some(Value::I32(expected))
        );
    }
    for (opcode, value, expected) in [(0x79, 0i64, 64), (0x7a, 16, 4), (0x7b, 0xff00ff, 16)] {
        let bytes = module(&[I64], I64, &[0x20, 0x00, opcode]);
        assert_eq!(
            instance(&bytes).invoke_export("run", &[Value::I64(value)]).unwrap(),
            Some(Value::I64(expected))
        );
    }
}

#[test]
fn signed_division_and_remainder_truncate_toward_zero() {
    assert_eq!(binary_i32(0x6d, -7, 3).unwrap(), Some(Value::I32(-2)));
    assert_eq!(binary_i32(0x6f, -7, 3).unwrap(), Some(Value::I32(-1)));
    assert_eq!(binary_i64(0x7f, -7, 3).unwrap(), Some(Value::I64(-2)));
    assert_eq!(binary_i64(0x81, -7, 3).unwrap(), Some(Value::I64(-1)));
}

#[test]
fn unsigned_division_and_remainder_use_unsigned_views() {
    assert_eq!(binary_i32(0x6e, -1, 2).unwrap(), Some(Value::I32(i32::MAX)));
    assert_eq!(binary_i32(0x70, -1, 2).unwrap(), Some(Value::I32(1)));
    assert_eq!(binary_i64(0x80, -1, 2).unwrap(), Some(Value::I64(i64::MAX)));
    assert_eq!(binary_i64(0x82, -1, 2).unwrap(), Some(Value::I64(1)));
}

#[test]
fn integer_divide_and_remainder_by_zero_trap() {
    for opcode in [0x6d, 0x6e, 0x6f, 0x70] {
        assert!(matches!(
            binary_i32(opcode, 7, 0),
            Err(RuntimeError::IntegerDivisionByZero)
        ));
    }
    for opcode in [0x7f, 0x80, 0x81, 0x82] {
        assert!(matches!(
            binary_i64(opcode, 7, 0),
            Err(RuntimeError::IntegerDivisionByZero)
        ));
    }
}

#[test]
fn signed_division_min_by_minus_one_traps_but_remainder_is_zero() {
    assert!(matches!(
        binary_i32(0x6d, i32::MIN, -1),
        Err(RuntimeError::IntegerOverflow)
    ));
    assert_eq!(binary_i32(0x6f, i32::MIN, -1).unwrap(), Some(Value::I32(0)));
    assert!(matches!(
        binary_i64(0x7f, i64::MIN, -1),
        Err(RuntimeError::IntegerOverflow)
    ));
    assert_eq!(binary_i64(0x81, i64::MIN, -1).unwrap(), Some(Value::I64(0)));
}

#[test]
fn bitwise_operators_cover_both_integer_widths() {
    assert_eq!(binary_i32(0x71, 0b1100, 0b1010).unwrap(), Some(Value::I32(0b1000)));
    assert_eq!(binary_i32(0x72, 0b1100, 0b1010).unwrap(), Some(Value::I32(0b1110)));
    assert_eq!(binary_i32(0x73, 0b1100, 0b1010).unwrap(), Some(Value::I32(0b0110)));
    assert_eq!(binary_i64(0x83, 0x0f0f, 0x00ff).unwrap(), Some(Value::I64(0x000f)));
    assert_eq!(binary_i64(0x84, 0x0f00, 0x00f0).unwrap(), Some(Value::I64(0x0ff0)));
    assert_eq!(binary_i64(0x85, 0x0ff0, 0x00ff).unwrap(), Some(Value::I64(0x0f0f)));
}

#[test]
fn shifts_mask_counts_and_preserve_signedness() {
    assert_eq!(binary_i32(0x74, 1, 33).unwrap(), Some(Value::I32(2)));
    assert_eq!(binary_i32(0x75, -8, 33).unwrap(), Some(Value::I32(-4)));
    assert_eq!(binary_i32(0x76, -1, 1).unwrap(), Some(Value::I32(i32::MAX)));
    assert_eq!(binary_i64(0x86, 1, 65).unwrap(), Some(Value::I64(2)));
    assert_eq!(binary_i64(0x87, -8, 65).unwrap(), Some(Value::I64(-4)));
    assert_eq!(binary_i64(0x88, -1, 1).unwrap(), Some(Value::I64(i64::MAX)));
}

#[test]
fn rotate_counts_are_masked_for_i32_and_i64() {
    assert_eq!(
        binary_i32(0x77, 0x4000_0001, 33).unwrap(),
        Some(Value::I32(i32::from_ne_bytes(0x8000_0002u32.to_ne_bytes())))
    );
    assert_eq!(
        binary_i32(0x78, 0x0000_0003, 33).unwrap(),
        Some(Value::I32(i32::from_ne_bytes(0x8000_0001u32.to_ne_bytes())))
    );
    assert_eq!(binary_i64(0x89, 1, 65).unwrap(), Some(Value::I64(2)));
    assert_eq!(
        binary_i64(0x8a, 2, 65).unwrap(),
        Some(Value::I64(1))
    );
}

#[test]
fn integer_operators_execute_inside_structured_control() {
    let bytes = module(
        &[I32, I32],
        I32,
        &[0x02, I32, 0x20, 0x00, 0x20, 0x01, 0x71, 0x0b],
    );
    assert_eq!(
        instance(&bytes)
            .invoke_export("run", &[Value::I32(0b1100), Value::I32(0b1010)])
            .unwrap(),
        Some(Value::I32(0b1000))
    );
}

#[test]
fn validator_rejects_integer_operator_type_confusion() {
    let bytes = module(&[I64, I64], I64, &[0x20, 0x00, 0x20, 0x01, 0x6d]);
    let error = Instance::new(parse_module(&bytes).unwrap()).unwrap_err();
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::TypeMismatch {
            expected: ValueType::I32,
            actual: ValueType::I64,
            ..
        })
    ));
}
''')
