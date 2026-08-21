use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};

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

fn function_module(params: &[u8], result: u8, instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    let mut ty = vec![0x01, 0x60];
    push_u32(&mut ty, params.len() as u32);
    ty.extend_from_slice(params);
    ty.extend_from_slice(&[0x01, result]);
    push_section(&mut module, 1, &ty);
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

fn invoke(bytes: &[u8], args: &[Value]) -> Result<Option<Value>, RuntimeError> {
    let mut instance =
        Instance::new(parse_module(bytes).expect("translated spec vector must parse"))?;
    instance.invoke_export("run", args)
}

fn invoke_i32(opcode: u8, a: i32, b: i32) -> Result<i32, RuntimeError> {
    let bytes = function_module(&[I32, I32], I32, &[0x20, 0x00, 0x20, 0x01, opcode]);
    match invoke(&bytes, &[Value::I32(a), Value::I32(b)])? {
        Some(Value::I32(value)) => Ok(value),
        other => panic!("translated i32 vector returned wrong type: {other:?}"),
    }
}

fn invoke_i64(opcode: u8, a: i64, b: i64) -> Result<i64, RuntimeError> {
    let bytes = function_module(&[I64, I64], I64, &[0x20, 0x00, 0x20, 0x01, opcode]);
    match invoke(&bytes, &[Value::I64(a), Value::I64(b)])? {
        Some(Value::I64(value)) => Ok(value),
        other => panic!("translated i64 vector returned wrong type: {other:?}"),
    }
}

#[test]
fn upstream_i32_arithmetic_vectors_cover_wrap_and_unsigned_views() {
    // WebAssembly/spec test/core/i32.wast @ fc209c5e...
    for (opcode, a, b, expected) in [
        (0x6a, i32::MAX, 1, i32::MIN),
        (0x6b, i32::MIN, 1, i32::MAX),
        (0x6c, 0x1000_0000, 4096, 0),
        (0x6e, i32::MIN, 2, 0x4000_0000),
        (0x70, -5, 2, 0x7fff_fffd),
    ] {
        assert_eq!(invoke_i32(opcode, a, b).unwrap(), expected);
    }
}

#[test]
fn upstream_i32_trap_and_remainder_vectors_keep_exact_classes() {
    // WebAssembly/spec test/core/i32.wast @ fc209c5e...
    assert!(matches!(
        invoke_i32(0x6d, 1, 0),
        Err(RuntimeError::IntegerDivisionByZero)
    ));
    assert!(matches!(
        invoke_i32(0x6d, i32::MIN, -1),
        Err(RuntimeError::IntegerOverflow)
    ));
    assert_eq!(invoke_i32(0x6f, i32::MIN, -1).unwrap(), 0);
    assert_eq!(invoke_i32(0x6f, -7, 3).unwrap(), -1);
}

#[test]
fn upstream_shift_rotate_vectors_mask_counts_by_integer_width() {
    // WebAssembly/spec integer operator assertions @ fc209c5e...
    assert_eq!(invoke_i32(0x74, 1, 32).unwrap(), 1);
    assert_eq!(invoke_i32(0x74, 1, 33).unwrap(), 2);
    assert_eq!(invoke_i32(0x77, 1, 33).unwrap(), 2);
    assert_eq!(invoke_i32(0x78, 2, 33).unwrap(), 1);

    assert_eq!(invoke_i64(0x86, 1, 64).unwrap(), 1);
    assert_eq!(invoke_i64(0x86, 1, 65).unwrap(), 2);
    assert_eq!(invoke_i64(0x89, 1, 65).unwrap(), 2);
    assert_eq!(invoke_i64(0x8a, 2, 65).unwrap(), 1);
}

#[test]
fn upstream_signed_and_unsigned_comparison_vectors_do_not_confuse_views() {
    // WebAssembly/spec i32/i64 comparison assertions @ fc209c5e...
    assert_eq!(invoke_i32(0x48, -1, 1).unwrap(), 1); // i32.lt_s
    assert_eq!(invoke_i32(0x49, -1, 1).unwrap(), 0); // i32.lt_u
    assert_eq!(invoke_i32(0x4a, -1, 1).unwrap(), 0); // i32.gt_s
    assert_eq!(invoke_i32(0x4b, -1, 1).unwrap(), 1); // i32.gt_u

    let lt_s = function_module(&[I64, I64], I32, &[0x20, 0x00, 0x20, 0x01, 0x53]);
    let lt_u = function_module(&[I64, I64], I32, &[0x20, 0x00, 0x20, 0x01, 0x54]);
    assert_eq!(
        invoke(&lt_s, &[Value::I64(-1), Value::I64(1)]).unwrap(),
        Some(Value::I32(1))
    );
    assert_eq!(
        invoke(&lt_u, &[Value::I64(-1), Value::I64(1)]).unwrap(),
        Some(Value::I32(0))
    );
}

#[test]
fn upstream_float_rounding_vectors_preserve_ties_and_signed_zero() {
    // WebAssembly/spec f32/f64 unary operator assertions @ fc209c5e...
    let f32_nearest = function_module(&[F32], F32, &[0x20, 0x00, 0x90]);
    let f64_nearest = function_module(&[F64], F64, &[0x20, 0x00, 0x9e]);

    for (input, expected) in [(2.5f32, 2.0f32), (3.5, 4.0), (-2.5, -2.0)] {
        assert_eq!(
            invoke(&f32_nearest, &[Value::F32(input)]).unwrap(),
            Some(Value::F32(expected))
        );
    }

    let negative_zero = invoke(&f64_nearest, &[Value::F64(-0.25)])
        .unwrap()
        .expect("f64.nearest returns one value");
    let Value::F64(value) = negative_zero else {
        panic!("f64.nearest returned wrong value type: {negative_zero:?}");
    };
    assert_eq!(value.to_bits(), (-0.0f64).to_bits());
}

#[test]
fn upstream_conversion_vectors_cover_unsigned_extension_and_wrap() {
    // WebAssembly/spec conversion assertions @ fc209c5e...
    let wrap = function_module(&[I64], I32, &[0x20, 0x00, 0xa7]);
    let extend_u = function_module(&[I32], I64, &[0x20, 0x00, 0xad]);

    assert_eq!(
        invoke(&wrap, &[Value::I64(0x0000_0001_ffff_ffff)]).unwrap(),
        Some(Value::I32(-1))
    );
    assert_eq!(
        invoke(&extend_u, &[Value::I32(-1)]).unwrap(),
        Some(Value::I64(0xffff_ffff))
    );
}
