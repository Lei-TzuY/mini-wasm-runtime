use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, RuntimeError, Value};
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

fn push_name(bytes: &mut Vec<u8>, name: &str) {
    push_u32(bytes, name.len() as u32);
    bytes.extend_from_slice(name.as_bytes());
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

fn push_body(payload: &mut Vec<u8>, instructions: &[u8]) {
    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    push_u32(payload, body.len() as u32);
    payload.extend(body);
}

fn single_function_memory_module(
    params: &[u8],
    result: Option<u8>,
    instructions: &[u8],
) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let mut types = vec![0x01];
    push_func_type(&mut types, params, result);
    push_section(&mut module, 1, &types);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 5, &[0x01, 0x01, 0x01, 0x01]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);
    let mut code = vec![0x01];
    push_body(&mut code, instructions);
    push_section(&mut module, 10, &code);
    module
}

fn single_function_imported_memory_module(
    params: &[u8],
    result: Option<u8>,
    instructions: &[u8],
) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let mut types = vec![0x01];
    push_func_type(&mut types, params, result);
    push_section(&mut module, 1, &types);
    let mut imports = vec![0x01];
    push_name(&mut imports, "env");
    push_name(&mut imports, "mem");
    imports.extend([0x02, 0x01, 0x01, 0x01]);
    push_section(&mut module, 2, &imports);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);
    let mut code = vec![0x01];
    push_body(&mut code, instructions);
    push_section(&mut module, 10, &code);
    module
}

fn instance(bytes: &[u8]) -> Instance {
    Instance::new(parse_module(bytes).expect("parse numeric-memory fixture"))
        .expect("instantiate numeric-memory fixture")
}

fn store_then_load(store: u8, load: u8, alignment: u8) -> Vec<u8> {
    vec![
        0x20, 0x00, 0x20, 0x01, store, alignment, 0x00, 0x20, 0x00, load, alignment, 0x00,
    ]
}

#[test]
fn i64_full_width_memory_round_trip() {
    let code = store_then_load(0x37, 0x29, 0x03);
    let module = single_function_memory_module(&[I32, I64], Some(I64), &code);
    let mut vm = instance(&module);
    let value = 0x0102_0304_0506_0708i64;
    assert_eq!(
        vm.invoke_export("run", &[Value::I32(16), Value::I64(value)])
            .unwrap(),
        Some(Value::I64(value))
    );
}

#[test]
fn i64_narrow_stores_and_loads_truncate_then_extend() {
    for (store, signed_load, unsigned_load, alignment, unsigned_expected) in [
        (0x3c, 0x30, 0x31, 0x00, 0xffi64),
        (0x3d, 0x32, 0x33, 0x01, 0xffffi64),
        (0x3e, 0x34, 0x35, 0x02, 0xffff_ffffi64),
    ] {
        let signed = single_function_memory_module(
            &[I32, I64],
            Some(I64),
            &store_then_load(store, signed_load, alignment),
        );
        let mut signed_vm = instance(&signed);
        assert_eq!(
            signed_vm
                .invoke_export("run", &[Value::I32(24), Value::I64(-1)])
                .unwrap(),
            Some(Value::I64(-1))
        );

        let unsigned = single_function_memory_module(
            &[I32, I64],
            Some(I64),
            &store_then_load(store, unsigned_load, alignment),
        );
        let mut unsigned_vm = instance(&unsigned);
        assert_eq!(
            unsigned_vm
                .invoke_export("run", &[Value::I32(24), Value::I64(-1)])
                .unwrap(),
            Some(Value::I64(unsigned_expected))
        );
    }
}

#[test]
fn f32_memory_round_trip_preserves_nan_payload_bits() {
    let code = store_then_load(0x38, 0x2a, 0x02);
    let module = single_function_memory_module(&[I32, F32], Some(F32), &code);
    let mut vm = instance(&module);
    let bits = 0x7fc1_2345u32;
    let result = vm
        .invoke_export("run", &[Value::I32(32), Value::F32(f32::from_bits(bits))])
        .unwrap()
        .expect("f32 result");
    assert_eq!(result.as_f32().to_bits(), bits);
}

#[test]
fn f64_shared_memory_round_trip_is_bit_exact_and_little_endian() {
    let code = store_then_load(0x39, 0x2b, 0x03);
    let module = single_function_imported_memory_module(&[I32, F64], Some(F64), &code);
    let parsed = parse_module(&module).unwrap();
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let mut hosts = HostRegistry::new();
    hosts.register_memory("env", "mem", memory.clone()).unwrap();
    let mut vm = Instance::with_hosts(parsed, hosts).unwrap();
    let bits = 0x7ff8_0000_dead_beefu64;
    let result = vm
        .invoke_export("run", &[Value::I32(40), Value::F64(f64::from_bits(bits))])
        .unwrap()
        .expect("f64 result");
    assert_eq!(result.as_f64().to_bits(), bits);
    assert_eq!(memory.read(40, 8).unwrap(), bits.to_le_bytes());
}

#[test]
fn validator_rejects_wrong_numeric_store_value_type() {
    let module = single_function_memory_module(
        &[I32, I32],
        None,
        &[0x20, 0x00, 0x20, 0x01, 0x39, 0x03, 0x00],
    );
    let error = Instance::new(parse_module(&module).unwrap()).unwrap_err();
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::TypeMismatch {
            expected: wasm_parser::ValueType::F64,
            actual: wasm_parser::ValueType::I32,
            ..
        })
    ));
}

#[test]
fn validator_rejects_overaligned_numeric_memory_accesses() {
    let loads = [
        (0x29, I64, 3u8),
        (0x2a, F32, 2),
        (0x2b, F64, 3),
        (0x30, I64, 0),
        (0x31, I64, 0),
        (0x32, I64, 1),
        (0x33, I64, 1),
        (0x34, I64, 2),
        (0x35, I64, 2),
    ];
    for (opcode, result, maximum) in loads {
        let module = single_function_memory_module(
            &[I32],
            Some(result),
            &[0x20, 0x00, opcode, maximum + 1, 0x00],
        );
        let error = Instance::new(parse_module(&module).unwrap()).unwrap_err();
        assert!(matches!(
            error,
            RuntimeError::Validation(ValidationError::InvalidMemoryAlignment {
                alignment,
                maximum: actual_maximum,
                ..
            }) if alignment == u32::from(maximum + 1) && actual_maximum == u32::from(maximum)
        ));
    }

    let stores = [
        (0x37, I64, 3u8),
        (0x38, F32, 2),
        (0x39, F64, 3),
        (0x3c, I64, 0),
        (0x3d, I64, 1),
        (0x3e, I64, 2),
    ];
    for (opcode, value_type, maximum) in stores {
        let module = single_function_memory_module(
            &[I32, value_type],
            None,
            &[0x20, 0x00, 0x20, 0x01, opcode, maximum + 1, 0x00],
        );
        let error = Instance::new(parse_module(&module).unwrap()).unwrap_err();
        assert!(matches!(
            error,
            RuntimeError::Validation(ValidationError::InvalidMemoryAlignment {
                alignment,
                maximum: actual_maximum,
                ..
            }) if alignment == u32::from(maximum + 1) && actual_maximum == u32::from(maximum)
        ));
    }
}

#[test]
fn numeric_memory_oob_traps_report_exact_width() {
    let load = single_function_memory_module(&[I32], Some(I64), &[0x20, 0x00, 0x29, 0x03, 0x00]);
    let mut load_vm = instance(&load);
    assert!(matches!(
        load_vm.invoke_export("run", &[Value::I32(65_532)]),
        Err(RuntimeError::MemoryOutOfBounds {
            address: 65_532,
            width: 8
        })
    ));

    let store = single_function_memory_module(
        &[I32, F32],
        None,
        &[0x20, 0x00, 0x20, 0x01, 0x38, 0x02, 0x00],
    );
    let mut store_vm = instance(&store);
    assert!(matches!(
        store_vm.invoke_export("run", &[Value::I32(65_534), Value::F32(1.0)]),
        Err(RuntimeError::MemoryOutOfBounds {
            address: 65_534,
            width: 4
        })
    ));
}
