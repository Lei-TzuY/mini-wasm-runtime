use wasm_parser::{parse_module, ValueType};
use wasm_runtime::{Instance, RuntimeError};
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

fn push_func_type(payload: &mut Vec<u8>, params: &[u8], results: &[u8]) {
    payload.push(0x60);
    push_u32(payload, params.len() as u32);
    payload.extend_from_slice(params);
    push_u32(payload, results.len() as u32);
    payload.extend_from_slice(results);
}

fn push_body(code: &mut Vec<u8>, instructions: &[u8]) {
    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    push_u32(code, body.len() as u32);
    code.extend_from_slice(&body);
}

fn build_module(
    types: &[(&[u8], &[u8])],
    function_types: &[u32],
    bodies: &[&[u8]],
    table: Option<&[u8]>,
    memory: Option<&[u8]>,
    global: Option<&[u8]>,
) -> Vec<u8> {
    assert_eq!(function_types.len(), bodies.len());

    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    let mut type_section = Vec::new();
    push_u32(&mut type_section, types.len() as u32);
    for &(params, results) in types {
        push_func_type(&mut type_section, params, results);
    }
    push_section(&mut module, 1, &type_section);

    let mut function_section = Vec::new();
    push_u32(&mut function_section, function_types.len() as u32);
    for &type_index in function_types {
        push_u32(&mut function_section, type_index);
    }
    push_section(&mut module, 3, &function_section);

    if let Some(table) = table {
        push_section(&mut module, 4, table);
    }
    if let Some(memory) = memory {
        push_section(&mut module, 5, memory);
    }
    if let Some(global) = global {
        push_section(&mut module, 6, global);
    }

    let mut code = Vec::new();
    push_u32(&mut code, bodies.len() as u32);
    for body in bodies {
        push_body(&mut code, body);
    }
    push_section(&mut module, 10, &code);
    module
}

fn validation_error(module: &[u8], expectation: &str) -> ValidationError {
    let module = parse_module(module).expect("negative fixture must remain structurally parseable");
    match Instance::new(module).expect_err(expectation) {
        RuntimeError::Validation(error) => error,
        other => panic!("expected validator rejection, got {other:?}"),
    }
}

#[test]
fn global_set_value_type_must_match_global() {
    let mutable_i64_global = [
        0x01, // one global
        I64, 0x01, // mutable i64
        0x42, 0x00, 0x0b, // i64.const 0; end
    ];
    let instructions = [
        0x41, 0x00, // i32.const 0
        0x24, 0x00, // global.set 0
    ];
    let module = build_module(
        &[(&[], &[])],
        &[0],
        &[&instructions],
        None,
        None,
        Some(&mutable_i64_global),
    );

    assert!(matches!(
        validation_error(&module, "global.set must enforce the declared global type"),
        ValidationError::TypeMismatch {
            function: 0,
            expected: ValueType::I64,
            actual: ValueType::I32,
            ..
        }
    ));
}

#[test]
fn direct_call_argument_type_must_match_signature() {
    let target_body: [u8; 0] = [];
    let caller_body = [
        0x41, 0x00, // i32.const 0
        0x10, 0x00, // call function 0, which expects i64
    ];
    let module = build_module(
        &[(&[I64], &[]), (&[], &[])],
        &[0, 1],
        &[&target_body, &caller_body],
        None,
        None,
        None,
    );

    assert!(matches!(
        validation_error(&module, "call operands must match the callee signature"),
        ValidationError::TypeMismatch {
            function: 1,
            expected: ValueType::I64,
            actual: ValueType::I32,
            ..
        }
    ));
}

#[test]
fn call_indirect_selector_must_be_i32() {
    let table = [
        0x01, // one table
        0x70, // funcref
        0x00, 0x01, // min=1, no maximum
    ];
    let instructions = [
        0x43, 0x00, 0x00, 0x00, 0x00, // f32.const 0
        0x11, 0x00, 0x00, // call_indirect type 0, table 0
    ];
    let module = build_module(
        &[(&[], &[])],
        &[0],
        &[&instructions],
        Some(&table),
        None,
        None,
    );

    assert!(matches!(
        validation_error(&module, "call_indirect selector must be an i32"),
        ValidationError::TypeMismatch {
            function: 0,
            expected: ValueType::I32,
            actual: ValueType::F32,
            ..
        }
    ));
}

#[test]
fn call_indirect_argument_type_must_match_signature() {
    let table = [
        0x01, // one table
        0x70, // funcref
        0x00, 0x01, // min=1, no maximum
    ];
    let instructions = [
        0x41, 0x00, // i32.const 0: wrong argument for type 0's i64 parameter
        0x41, 0x00, // i32.const 0: selector
        0x11, 0x00, 0x00, // call_indirect type 0, table 0
    ];
    let module = build_module(
        &[(&[I64], &[]), (&[], &[])],
        &[1],
        &[&instructions],
        Some(&table),
        None,
        None,
    );

    assert!(matches!(
        validation_error(
            &module,
            "call_indirect operands must match the referenced type"
        ),
        ValidationError::TypeMismatch {
            function: 0,
            expected: ValueType::I64,
            actual: ValueType::I32,
            ..
        }
    ));
}

#[test]
fn memory_address_must_be_i32() {
    let memory = [
        0x01, // one memory
        0x00, 0x01, // min=1, no maximum
    ];
    let instructions = [
        0x42, 0x00, // i64.const 0: invalid address type
        0x28, 0x02, 0x00, // i32.load align=2, offset=0
    ];
    let module = build_module(
        &[(&[], &[])],
        &[0],
        &[&instructions],
        None,
        Some(&memory),
        None,
    );

    assert!(matches!(
        validation_error(&module, "linear-memory addresses must be i32"),
        ValidationError::TypeMismatch {
            function: 0,
            expected: ValueType::I32,
            actual: ValueType::I64,
            ..
        }
    ));
}

#[test]
fn memory_grow_delta_must_be_i32() {
    let memory = [
        0x01, // one memory
        0x00, 0x01, // min=1, no maximum
    ];
    let instructions = [
        0x42, 0x01, // i64.const 1: invalid page delta type
        0x40, 0x00, // memory.grow 0
    ];
    let module = build_module(
        &[(&[], &[])],
        &[0],
        &[&instructions],
        None,
        Some(&memory),
        None,
    );

    assert!(matches!(
        validation_error(&module, "memory.grow delta must be i32"),
        ValidationError::TypeMismatch {
            function: 0,
            expected: ValueType::I32,
            actual: ValueType::I64,
            ..
        }
    ));
}

#[test]
fn br_if_condition_must_be_i32() {
    let instructions = [
        0x02, 0x40, // block with empty signature
        0x42, 0x00, // i64.const 0: invalid branch condition type
        0x0d, 0x00, // br_if 0
        0x0b, // end block
    ];
    let module = build_module(
        &[(&[], &[])],
        &[0],
        &[&instructions],
        None,
        None,
        None,
    );

    assert!(matches!(
        validation_error(&module, "br_if condition must be i32"),
        ValidationError::TypeMismatch {
            function: 0,
            expected: ValueType::I32,
            actual: ValueType::I64,
            ..
        }
    ));
}
