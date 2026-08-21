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
    "                0xa7 | 0xac | 0xad | 0xb6 | 0xbb => numeric::convert(&mut stack, opcode)?,\n",
    "                0xa7 | 0xac | 0xad | 0xb6 | 0xbb | 0xbc..=0xbf => {\n                    numeric::convert(&mut stack, opcode)?\n                }\n",
    "runtime reinterpret dispatch",
)

replace_once(
    runtime,
    "            0x0f | 0x45..=0x66 | 0x67..=0x8a | 0x8b..=0xa6 | 0xa7 | 0xac | 0xad | 0xb6 | 0xbb => {}\n",
    "            0x0f\n            | 0x45..=0x66\n            | 0x67..=0x8a\n            | 0x8b..=0xa6\n            | 0xa7\n            | 0xac\n            | 0xad\n            | 0xb6\n            | 0xbb\n            | 0xbc..=0xbf => {}\n",
    "control map reinterpret range",
)

replace_once(
    numeric,
    '''        0xbb => {
            let value = match pop_typed(stack, ValueType::F32)? {
                Value::F32(value) => value,
                _ => unreachable!("pop_typed established f32"),
            };
            Value::F64(f64::from(value))
        }
        _ => return Err(RuntimeError::UnsupportedOpcode(opcode)),
''',
    '''        0xbb => {
            let value = match pop_typed(stack, ValueType::F32)? {
                Value::F32(value) => value,
                _ => unreachable!("pop_typed established f32"),
            };
            Value::F64(f64::from(value))
        }
        0xbc => Value::I32(f32_from_stack(stack)?.to_bits() as i32),
        0xbd => Value::I64(f64_from_stack(stack)?.to_bits() as i64),
        0xbe => Value::F32(f32::from_bits(i32_from_stack(stack)? as u32)),
        0xbf => Value::F64(f64::from_bits(i64_from_stack(stack)? as u64)),
        _ => return Err(RuntimeError::UnsupportedOpcode(opcode)),
''',
    "numeric reinterpret operations",
)

replace_once(
    validator,
    '''            0xbb => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::F32,
                    ValueType::F64,
                    function,
                    offset,
                )?;
            }
            other => {
''',
    '''            0xbb => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::F32,
                    ValueType::F64,
                    function,
                    offset,
                )?;
            }
            0xbc => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::F32,
                    ValueType::I32,
                    function,
                    offset,
                )?;
            }
            0xbd => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::F64,
                    ValueType::I64,
                    function,
                    offset,
                )?;
            }
            0xbe => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::I32,
                    ValueType::F32,
                    function,
                    offset,
                )?;
            }
            0xbf => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::I64,
                    ValueType::F64,
                    function,
                    offset,
                )?;
            }
            other => {
''',
    "validator reinterpret typing",
)

Path("crates/wasm-runtime/tests/phase5c_reinterpret.rs").write_text(r'''use wasm_parser::{parse_module, ValueType};
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

fn invoke(param: u8, result: u8, opcode: u8, value: Value) -> Value {
    let bytes = module(param, result, &[0x20, 0x00, opcode]);
    let mut instance = Instance::new(parse_module(&bytes).unwrap()).unwrap();
    instance.invoke_export("run", &[value]).unwrap().unwrap()
}

#[test]
fn f32_to_i32_reinterpret_preserves_every_bit() {
    for bits in [0x0000_0000u32, 0x8000_0000, 0x7fc1_2345, 0xff81_2345, 0xffff_ffff] {
        assert_eq!(
            invoke(F32, I32, 0xbc, Value::F32(f32::from_bits(bits))),
            Value::I32(bits as i32)
        );
    }
}

#[test]
fn i32_to_f32_reinterpret_preserves_every_bit() {
    for bits in [0x0000_0000u32, 0x8000_0000, 0x7fc1_2345, 0xff81_2345, 0xffff_ffff] {
        let result = invoke(I32, F32, 0xbe, Value::I32(bits as i32)).as_f32();
        assert_eq!(result.to_bits(), bits);
    }
}

#[test]
fn f64_to_i64_reinterpret_preserves_every_bit() {
    for bits in [
        0x0000_0000_0000_0000u64,
        0x8000_0000_0000_0000,
        0x7ff8_0000_dead_beef,
        0xfff0_0000_0000_0001,
        0xffff_ffff_ffff_ffff,
    ] {
        assert_eq!(
            invoke(F64, I64, 0xbd, Value::F64(f64::from_bits(bits))),
            Value::I64(bits as i64)
        );
    }
}

#[test]
fn i64_to_f64_reinterpret_preserves_every_bit() {
    for bits in [
        0x0000_0000_0000_0000u64,
        0x8000_0000_0000_0000,
        0x7ff8_0000_dead_beef,
        0xfff0_0000_0000_0001,
        0xffff_ffff_ffff_ffff,
    ] {
        let result = invoke(I64, F64, 0xbf, Value::I64(bits as i64)).as_f64();
        assert_eq!(result.to_bits(), bits);
    }
}

#[test]
fn reinterpret_round_trips_nan_payload_and_negative_zero() {
    let nan_bits = 0x7fc1_9876u32;
    let as_int = invoke(F32, I32, 0xbc, Value::F32(f32::from_bits(nan_bits))).as_i32();
    let back = invoke(I32, F32, 0xbe, Value::I32(as_int)).as_f32();
    assert_eq!(back.to_bits(), nan_bits);

    let negative_zero = invoke(I64, F64, 0xbf, Value::I64(i64::MIN)).as_f64();
    assert_eq!(negative_zero.to_bits(), 0x8000_0000_0000_0000);
}

#[test]
fn reinterpret_executes_inside_structured_control() {
    let bytes = module(F32, I32, &[0x02, I32, 0x20, 0x00, 0xbc, 0x0b]);
    let mut instance = Instance::new(parse_module(&bytes).unwrap()).unwrap();
    let result = instance
        .invoke_export("run", &[Value::F32(-0.0)])
        .unwrap()
        .unwrap();
    assert_eq!(result, Value::I32(i32::MIN));
}

#[test]
fn validator_rejects_reinterpret_type_confusion() {
    let bytes = module(I32, I32, &[0x20, 0x00, 0xbc]);
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
''')
