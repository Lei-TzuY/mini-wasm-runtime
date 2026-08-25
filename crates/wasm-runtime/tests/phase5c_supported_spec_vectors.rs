use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};

const I32: u8 = 0x7f;
const I64: u8 = 0x7e;
const F32: u8 = 0x7d;
const F64: u8 = 0x7c;
const UPSTREAM_SPEC_COMMIT: &str = "fc209c5ed8afc4dfeb9252024d217da3376c7a6f";

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

fn push_sleb(bytes: &mut Vec<u8>, mut value: i64) {
    loop {
        let mut byte = (value as u8) & 0x7f;
        let sign_bit_set = byte & 0x40 != 0;
        value >>= 7;
        let done = (value == 0 && !sign_bit_set) || (value == -1 && sign_bit_set);
        if !done {
            byte |= 0x80;
        }
        bytes.push(byte);
        if done {
            break;
        }
    }
}

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn push_i32_const(instructions: &mut Vec<u8>, value: i32) {
    instructions.push(0x41);
    push_sleb(instructions, value as i64);
}

fn push_i64_const(instructions: &mut Vec<u8>, value: i64) {
    instructions.push(0x42);
    push_sleb(instructions, value);
}

fn push_f32_const(instructions: &mut Vec<u8>, value: f32) {
    instructions.push(0x43);
    instructions.extend_from_slice(&value.to_le_bytes());
}

fn push_f64_const(instructions: &mut Vec<u8>, value: f64) {
    instructions.push(0x44);
    instructions.extend_from_slice(&value.to_le_bytes());
}

fn function_module(params: &[u8], result: u8, instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    let mut ty = vec![0x01, 0x60];
    push_u32(&mut ty, params.len() as u32);
    ty.extend_from_slice(params);
    ty.extend([0x01, result]);
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

fn invoke(module: &[u8], args: &[Value]) -> Option<Value> {
    let mut instance = Instance::new(parse_module(module).expect("translated spec vector must parse"))
        .expect("translated supported spec vector must validate and instantiate");
    instance
        .invoke_export("run", args)
        .expect("translated supported spec vector must execute")
}

fn invoke_i32_binary(opcode: u8, a: i32, b: i32) -> i32 {
    let module = function_module(&[I32, I32], I32, &[0x20, 0x00, 0x20, 0x01, opcode]);
    match invoke(&module, &[Value::I32(a), Value::I32(b)]) {
        Some(Value::I32(value)) => value,
        other => panic!("translated i32 vector returned wrong value: {other:?}"),
    }
}

#[test]
fn upstream_i32_wrap_vectors_match_core_spec() {
    // WebAssembly/spec test/core/i32.wast at the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);
    for (opcode, a, b, expected) in [
        (0x6a, i32::MAX, 1, i32::MIN),
        (0x6a, i32::MIN, -1, i32::MAX),
        (0x6b, i32::MIN, 1, i32::MAX),
        (0x6c, 0x1000_0000, 4096, 0),
        (0x6c, i32::MIN, -1, i32::MIN),
    ] {
        assert_eq!(invoke_i32_binary(opcode, a, b), expected);
    }
}

#[test]
fn upstream_func_i32_value_return_and_break_vectors_execute() {
    // WebAssembly/spec test/core/func.wast: value-i32, return-i32, break-i32.
    let mut value = Vec::new();
    push_i32_const(&mut value, 77);
    assert_eq!(invoke(&function_module(&[], I32, &value), &[]), Some(Value::I32(77)));

    let mut returned = Vec::new();
    push_i32_const(&mut returned, 78);
    returned.push(0x0f); // return
    assert_eq!(
        invoke(&function_module(&[], I32, &returned), &[]),
        Some(Value::I32(78))
    );

    let mut branched = Vec::new();
    push_i32_const(&mut branched, 79);
    branched.extend([0x0c, 0x00]); // br 0 to the function label
    assert_eq!(
        invoke(&function_module(&[], I32, &branched), &[]),
        Some(Value::I32(79))
    );
}

#[test]
fn upstream_func_non_i32_return_vectors_execute() {
    // WebAssembly/spec test/core/func.wast: return-i64, return-f32, return-f64.
    let mut i64_result = Vec::new();
    push_i64_const(&mut i64_result, 7878);
    i64_result.push(0x0f);
    assert_eq!(
        invoke(&function_module(&[], I64, &i64_result), &[]),
        Some(Value::I64(7878))
    );

    let mut f32_result = Vec::new();
    push_f32_const(&mut f32_result, 78.7);
    f32_result.push(0x0f);
    assert_eq!(
        invoke(&function_module(&[], F32, &f32_result), &[]),
        Some(Value::F32(78.7))
    );

    let mut f64_result = Vec::new();
    push_f64_const(&mut f64_result, 78.78);
    f64_result.push(0x0f);
    assert_eq!(
        invoke(&function_module(&[], F64, &f64_result), &[]),
        Some(Value::F64(78.78))
    );
}

#[test]
fn upstream_func_non_i32_break_vectors_execute() {
    // WebAssembly/spec test/core/func.wast: break-i64, break-f32, break-f64.
    let mut i64_result = Vec::new();
    push_i64_const(&mut i64_result, 7979);
    i64_result.extend([0x0c, 0x00]);
    assert_eq!(
        invoke(&function_module(&[], I64, &i64_result), &[]),
        Some(Value::I64(7979))
    );

    let mut f32_result = Vec::new();
    push_f32_const(&mut f32_result, 79.9);
    f32_result.extend([0x0c, 0x00]);
    assert_eq!(
        invoke(&function_module(&[], F32, &f32_result), &[]),
        Some(Value::F32(79.9))
    );

    let mut f64_result = Vec::new();
    push_f64_const(&mut f64_result, 79.79);
    f64_result.extend([0x0c, 0x00]);
    assert_eq!(
        invoke(&function_module(&[], F64, &f64_result), &[]),
        Some(Value::F64(79.79))
    );
}
