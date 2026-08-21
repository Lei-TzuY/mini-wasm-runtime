use std::{cell::Cell, rc::Rc};

use wasm_parser::parse_module;
use wasm_runtime::{HostCapabilities, HostRegistry, Instance, RuntimeError, Value};
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

fn push_func_type(payload: &mut Vec<u8>, params: &[u8], result: Option<u8>) {
    payload.push(0x60);
    push_u32(payload, params.len() as u32);
    payload.extend_from_slice(params);
    match result {
        Some(result) => payload.extend([0x01, result]),
        None => payload.push(0x00),
    }
}

fn push_body(payload: &mut Vec<u8>, locals: &[(u32, u8)], instructions: &[u8]) {
    let mut body = Vec::new();
    push_u32(&mut body, locals.len() as u32);
    for &(count, ty) in locals {
        push_u32(&mut body, count);
        body.push(ty);
    }
    body.extend_from_slice(instructions);
    body.push(0x0b);
    push_u32(payload, body.len() as u32);
    payload.extend(body);
}

fn single_function_module(
    params: &[u8],
    result: Option<u8>,
    locals: &[(u32, u8)],
    instructions: &[u8],
) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let mut types = vec![0x01];
    push_func_type(&mut types, params, result);
    push_section(&mut module, 1, &types);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(
        &mut module,
        7,
        &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00],
    );
    let mut code = vec![0x01];
    push_body(&mut code, locals, instructions);
    push_section(&mut module, 10, &code);
    module
}

fn instance(bytes: &[u8]) -> Instance {
    Instance::new(parse_module(bytes).expect("parse fixture")).expect("instantiate fixture")
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

#[test]
fn executes_i64_wrapping_arithmetic() {
    let module = single_function_module(
        &[I64, I64],
        Some(I64),
        &[],
        &[0x20, 0x00, 0x20, 0x01, 0x7c],
    );
    let mut vm = instance(&module);
    assert_eq!(
        vm.invoke_export("run", &[Value::I64(i64::MAX), Value::I64(1)])
            .unwrap(),
        Some(Value::I64(i64::MIN))
    );
}

#[test]
fn executes_f32_and_f64_arithmetic() {
    let f32_module = single_function_module(
        &[F32, F32],
        Some(F32),
        &[],
        &[0x20, 0x00, 0x20, 0x01, 0x94],
    );
    let mut f32_vm = instance(&f32_module);
    assert_eq!(
        f32_vm
            .invoke_export("run", &[Value::F32(1.5), Value::F32(4.0)])
            .unwrap(),
        Some(Value::F32(6.0))
    );

    let f64_module = single_function_module(
        &[F64, F64],
        Some(F64),
        &[],
        &[0x20, 0x00, 0x20, 0x01, 0xa3],
    );
    let mut f64_vm = instance(&f64_module);
    assert_eq!(
        f64_vm
            .invoke_export("run", &[Value::F64(9.0), Value::F64(4.5)])
            .unwrap(),
        Some(Value::F64(2.0))
    );
}

#[test]
fn distinguishes_signed_and_unsigned_integer_comparisons() {
    let signed = single_function_module(
        &[I32],
        Some(I32),
        &[],
        &[0x20, 0x00, 0x41, 0x01, 0x48],
    );
    let unsigned = single_function_module(
        &[I32],
        Some(I32),
        &[],
        &[0x20, 0x00, 0x41, 0x01, 0x49],
    );
    assert_eq!(
        instance(&signed)
            .invoke_export("run", &[Value::I32(-1)])
            .unwrap(),
        Some(Value::I32(1))
    );
    assert_eq!(
        instance(&unsigned)
            .invoke_export("run", &[Value::I32(-1)])
            .unwrap(),
        Some(Value::I32(0))
    );

    let i64_unsigned = single_function_module(
        &[I64],
        Some(I32),
        &[],
        &[0x20, 0x00, 0x42, 0x01, 0x54],
    );
    assert_eq!(
        instance(&i64_unsigned)
            .invoke_export("run", &[Value::I64(-1)])
            .unwrap(),
        Some(Value::I32(0))
    );
}

#[test]
fn float_nan_comparisons_follow_ieee_rules() {
    let eq = single_function_module(
        &[F32, F32],
        Some(I32),
        &[],
        &[0x20, 0x00, 0x20, 0x01, 0x5b],
    );
    let ne = single_function_module(
        &[F64, F64],
        Some(I32),
        &[],
        &[0x20, 0x00, 0x20, 0x01, 0x62],
    );
    assert_eq!(
        instance(&eq)
            .invoke_export("run", &[Value::F32(f32::NAN), Value::F32(f32::NAN)])
            .unwrap(),
        Some(Value::I32(0))
    );
    assert_eq!(
        instance(&ne)
            .invoke_export("run", &[Value::F64(f64::NAN), Value::F64(f64::NAN)])
            .unwrap(),
        Some(Value::I32(1))
    );
}

#[test]
fn executes_selected_non_trapping_conversions() {
    let wrap = single_function_module(&[I64], Some(I32), &[], &[0x20, 0x00, 0xa7]);
    assert_eq!(
        instance(&wrap)
            .invoke_export("run", &[Value::I64(0x1_0000_0001)])
            .unwrap(),
        Some(Value::I32(1))
    );

    let extend_s = single_function_module(&[I32], Some(I64), &[], &[0x20, 0x00, 0xac]);
    let extend_u = single_function_module(&[I32], Some(I64), &[], &[0x20, 0x00, 0xad]);
    assert_eq!(
        instance(&extend_s)
            .invoke_export("run", &[Value::I32(-1)])
            .unwrap(),
        Some(Value::I64(-1))
    );
    assert_eq!(
        instance(&extend_u)
            .invoke_export("run", &[Value::I32(-1)])
            .unwrap(),
        Some(Value::I64(4_294_967_295))
    );

    let demote = single_function_module(&[F64], Some(F32), &[], &[0x20, 0x00, 0xb6]);
    let promote = single_function_module(&[F32], Some(F64), &[], &[0x20, 0x00, 0xbb]);
    assert_eq!(
        instance(&demote)
            .invoke_export("run", &[Value::F64(1.25)])
            .unwrap(),
        Some(Value::F32(1.25))
    );
    assert_eq!(
        instance(&promote)
            .invoke_export("run", &[Value::F32(1.25)])
            .unwrap(),
        Some(Value::F64(1.25))
    );
}

#[test]
fn typed_locals_zero_initialize_by_declared_type() {
    for (ty, expected) in [
        (I64, Value::I64(0)),
        (F32, Value::F32(0.0)),
        (F64, Value::F64(0.0)),
    ] {
        let module = single_function_module(&[], Some(ty), &[(1, ty)], &[0x20, 0x00]);
        assert_eq!(instance(&module).invoke_export("run", &[]).unwrap(), Some(expected));
    }
}

#[test]
fn typed_global_state_persists() {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let mut types = vec![0x01];
    push_func_type(&mut types, &[], Some(I64));
    push_section(&mut module, 1, &types);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 6, &[0x01, I64, 0x01, 0x42, 0x01, 0x0b]);
    push_section(
        &mut module,
        7,
        &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00],
    );
    let mut code = vec![0x01];
    push_body(
        &mut code,
        &[],
        &[0x23, 0x00, 0x42, 0x01, 0x7c, 0x24, 0x00, 0x23, 0x00],
    );
    push_section(&mut module, 10, &code);

    let mut vm = instance(&module);
    assert_eq!(vm.global(0), Some(Value::I64(1)));
    assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I64(2)));
    assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I64(3)));
}

#[test]
fn typed_block_result_is_preserved() {
    let mut instructions = vec![0x02, F64];
    instructions.extend(f64_const(3.5));
    instructions.push(0x0b);
    let module = single_function_module(&[], Some(F64), &[], &instructions);
    assert_eq!(
        instance(&module).invoke_export("run", &[]).unwrap(),
        Some(Value::F64(3.5))
    );
}

#[test]
fn direct_call_supports_non_i32_signature() {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let mut types = vec![0x01];
    push_func_type(&mut types, &[F64], Some(F64));
    push_section(&mut module, 1, &types);
    push_section(&mut module, 3, &[0x02, 0x00, 0x00]);
    push_section(
        &mut module,
        7,
        &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x01],
    );
    let mut code = vec![0x02];
    let mut target = vec![0x20, 0x00];
    target.extend(f64_const(2.0));
    target.push(0xa2);
    push_body(&mut code, &[], &target);
    push_body(&mut code, &[], &[0x20, 0x00, 0x10, 0x00]);
    push_section(&mut module, 10, &code);

    assert_eq!(
        instance(&module)
            .invoke_export("run", &[Value::F64(4.25)])
            .unwrap(),
        Some(Value::F64(8.5))
    );
}

#[test]
fn indirect_call_supports_non_i32_signature() {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let mut types = vec![0x02];
    push_func_type(&mut types, &[F64], Some(F64));
    push_func_type(&mut types, &[F64, I32], Some(F64));
    push_section(&mut module, 1, &types);
    push_section(&mut module, 3, &[0x02, 0x00, 0x01]);
    push_section(&mut module, 4, &[0x01, 0x70, 0x00, 0x01]);
    push_section(
        &mut module,
        7,
        &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x01],
    );
    push_section(&mut module, 9, &[0x01, 0x00, 0x41, 0x00, 0x0b, 0x01, 0x00]);
    let mut code = vec![0x02];
    let mut target = vec![0x20, 0x00];
    target.extend(f64_const(2.0));
    target.push(0xa0);
    push_body(&mut code, &[], &target);
    push_body(
        &mut code,
        &[],
        &[0x20, 0x00, 0x20, 0x01, 0x11, 0x00, 0x00],
    );
    push_section(&mut module, 10, &code);

    assert_eq!(
        instance(&module)
            .invoke_export("run", &[Value::F64(40.0), Value::I32(0)])
            .unwrap(),
        Some(Value::F64(42.0))
    );
}

#[test]
fn validator_rejects_numeric_type_confusion() {
    let wrong_add = single_function_module(
        &[],
        Some(I32),
        &[],
        &[0x42, 0x01, 0x41, 0x01, 0x6a],
    );
    let error = Instance::new(parse_module(&wrong_add).unwrap()).expect_err("mixed add must fail");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::TypeMismatch {
            expected: wasm_parser::ValueType::I32,
            actual: wasm_parser::ValueType::I64,
            ..
        })
    ));

    let mut wrong_local = f32_const(1.0);
    wrong_local.extend([0x21, 0x00]);
    let wrong_local = single_function_module(&[], None, &[(1, I64)], &wrong_local);
    assert!(matches!(
        Instance::new(parse_module(&wrong_local).unwrap()),
        Err(RuntimeError::Validation(ValidationError::TypeMismatch {
            expected: wasm_parser::ValueType::I64,
            actual: wasm_parser::ValueType::F32,
            ..
        }))
    ));
}

#[test]
fn runtime_rejects_wrong_argument_variant_before_execution() {
    let module = single_function_module(&[I64], Some(I64), &[], &[0x20, 0x00]);
    let error = instance(&module)
        .invoke_export("run", &[Value::I32(7)])
        .expect_err("wrong runtime variant must be rejected");
    assert!(matches!(
        error,
        RuntimeError::ValueTypeMismatch {
            expected: wasm_parser::ValueType::I64,
            actual: wasm_parser::ValueType::I32,
        }
    ));
}

#[test]
fn wrong_host_argument_variant_is_rejected_before_callback() {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let mut types = vec![0x01];
    push_func_type(&mut types, &[I32], Some(I32));
    push_section(&mut module, 1, &types);
    push_section(
        &mut module,
        2,
        &[
            0x01, 0x03, b'e', b'n', b'v', 0x01, b'f', 0x00, 0x00,
        ],
    );
    push_section(
        &mut module,
        7,
        &[0x01, 0x01, b'f', 0x00, 0x00],
    );
    let module = parse_module(&module).unwrap();

    let called = Rc::new(Cell::new(false));
    let callback_called = called.clone();
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "f",
            vec![wasm_parser::ValueType::I32],
            vec![wasm_parser::ValueType::I32],
            HostCapabilities::NONE,
            move |_ctx, _args| {
                callback_called.set(true);
                Ok(Some(Value::I32(1)))
            },
        )
        .unwrap();
    let mut vm = Instance::with_hosts(module, hosts).unwrap();
    let error = vm
        .invoke_export("f", &[Value::I64(7)])
        .expect_err("wrong host argument type must fail before callback");
    assert!(!called.get());
    assert!(matches!(
        error,
        RuntimeError::ValueTypeMismatch {
            expected: wasm_parser::ValueType::I32,
            actual: wasm_parser::ValueType::I64,
        }
    ));
}
