use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};

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

fn function_module(params: &[u8], result: u8, instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    let mut ty = vec![0x01, 0x60];
    push_u32(&mut ty, params.len() as u32);
    ty.extend_from_slice(params);
    ty.extend_from_slice(&[0x01, result]);
    push_section(&mut module, 1, &ty);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(
        &mut module,
        7,
        &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00],
    );

    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);

    module
}

fn memory_grow_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x01, I32]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 5, &[0x01, 0x01, 0x01, 0x01]);
    push_section(
        &mut module,
        7,
        &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00],
    );

    let body = [0x00, 0x41, 0x01, 0x40, 0x00, 0x0b];
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn invoke(bytes: &[u8], args: &[Value]) -> Result<Option<Value>, RuntimeError> {
    let mut instance = Instance::new(parse_module(bytes).expect("spec fixture must parse"))?;
    instance.invoke_export("run", args)
}

#[test]
fn spec_integer_division_and_remainder_truncate_toward_zero() {
    for (opcode, expected) in [(0x6d, -2), (0x6f, -1)] {
        let bytes = function_module(&[I32, I32], I32, &[0x20, 0x00, 0x20, 0x01, opcode]);
        assert_eq!(
            invoke(&bytes, &[Value::I32(-7), Value::I32(3)]).unwrap(),
            Some(Value::I32(expected))
        );
    }
}

#[test]
fn spec_integer_shift_counts_are_reduced_modulo_bit_width() {
    let bytes = function_module(&[I32, I32], I32, &[0x20, 0x00, 0x20, 0x01, 0x74]);

    assert_eq!(
        invoke(&bytes, &[Value::I32(1), Value::I32(32)]).unwrap(),
        Some(Value::I32(1))
    );
    assert_eq!(
        invoke(&bytes, &[Value::I32(1), Value::I32(33)]).unwrap(),
        Some(Value::I32(2))
    );
}

#[test]
fn spec_float_min_max_select_signed_zero_deterministically() {
    for (opcode, expected_bits) in [(0x96, (-0.0f32).to_bits()), (0x97, 0.0f32.to_bits())] {
        let bytes = function_module(&[F32, F32], F32, &[0x20, 0x00, 0x20, 0x01, opcode]);
        let result = invoke(&bytes, &[Value::F32(0.0), Value::F32(-0.0)])
            .unwrap()
            .expect("float min/max returns one value");
        let Value::F32(value) = result else {
            panic!("float min/max returned wrong value type: {result:?}");
        };
        assert_eq!(value.to_bits(), expected_bits);
    }
}

#[test]
fn spec_trapping_conversion_distinguishes_nan_from_numeric_overflow() {
    let bytes = function_module(&[F64], I32, &[0x20, 0x00, 0xaa]);

    assert!(matches!(
        invoke(&bytes, &[Value::F64(f64::NAN)]),
        Err(RuntimeError::InvalidConversionToInteger)
    ));
    assert!(matches!(
        invoke(&bytes, &[Value::F64(f64::INFINITY)]),
        Err(RuntimeError::IntegerOverflow)
    ));
}

#[test]
fn spec_unsigned_truncation_accepts_negative_fraction_that_becomes_zero() {
    let bytes = function_module(&[F64], I32, &[0x20, 0x00, 0xab]);

    assert_eq!(
        invoke(&bytes, &[Value::F64(-0.999)]).unwrap(),
        Some(Value::I32(0))
    );
}

#[test]
fn spec_memory_grow_at_declared_max_returns_minus_one() {
    let bytes = memory_grow_module();

    assert_eq!(invoke(&bytes, &[]).unwrap(), Some(Value::I32(-1)));
}
