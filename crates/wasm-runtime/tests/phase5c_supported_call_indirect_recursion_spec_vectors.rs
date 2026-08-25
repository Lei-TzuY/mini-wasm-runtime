use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};

const I32: u8 = 0x7f;
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

fn push_name(bytes: &mut Vec<u8>, name: &str) {
    push_u32(bytes, name.len() as u32);
    bytes.extend_from_slice(name.as_bytes());
}

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn push_body(payload: &mut Vec<u8>, instructions: &[u8]) {
    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    push_u32(payload, body.len() as u32);
    payload.extend_from_slice(&body);
}

fn push_export(payload: &mut Vec<u8>, name: &str, function_index: u8) {
    push_name(payload, name);
    payload.extend([0x00, function_index]);
}

fn recursive_call_indirect_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(&mut module, 1, &[0x01, 0x60, 0x01, I32, 0x01, I32]);
    push_section(&mut module, 3, &[0x02, 0x00, 0x00]);
    push_section(&mut module, 4, &[0x01, 0x70, 0x00, 0x02]);

    let mut exports = vec![0x02];
    push_export(&mut exports, "fac-i32", 0);
    push_export(&mut exports, "fib-i32", 1);
    push_section(&mut module, 7, &exports);

    push_section(
        &mut module,
        9,
        &[0x01, 0x00, 0x41, 0x00, 0x0b, 0x02, 0x00, 0x01],
    );

    let factorial = [
        0x20, 0x00, 0x45, 0x04, I32, 0x41, 0x01, 0x05, 0x20, 0x00, 0x20, 0x00, 0x41, 0x01,
        0x6b, 0x41, 0x00, 0x11, 0x00, 0x00, 0x6c, 0x0b,
    ];
    let fibonacci = [
        0x20, 0x00, 0x41, 0x01, 0x4d, 0x04, I32, 0x41, 0x01, 0x05, 0x20, 0x00, 0x41, 0x02,
        0x6b, 0x41, 0x01, 0x11, 0x00, 0x00, 0x20, 0x00, 0x41, 0x01, 0x6b, 0x41, 0x01, 0x11, 0x00,
        0x00, 0x6a, 0x0b,
    ];

    let mut code = vec![0x02];
    push_body(&mut code, &factorial);
    push_body(&mut code, &fibonacci);
    push_section(&mut module, 10, &code);
    module
}

fn instance() -> Instance {
    Instance::new(
        parse_module(&recursive_call_indirect_module())
            .expect("recursive call_indirect spec vector must parse"),
    )
    .expect("recursive call_indirect spec vector must validate and instantiate")
}

fn invoke_i32(vm: &mut Instance, name: &str, argument: i32) -> i32 {
    match vm
        .invoke_export(name, &[Value::I32(argument)])
        .expect("recursive call_indirect spec vector must execute")
    {
        Some(Value::I32(value)) => value,
        other => panic!("recursive call_indirect spec vector returned wrong value: {other:?}"),
    }
}

#[test]
fn upstream_i32_factorial_recurses_through_table_slot() {
    // WebAssembly/spec test/core/call_indirect.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let mut vm = instance();
    assert_eq!(invoke_i32(&mut vm, "fac-i32", 0), 1);
    assert_eq!(invoke_i32(&mut vm, "fac-i32", 1), 1);
    assert_eq!(invoke_i32(&mut vm, "fac-i32", 5), 120);
    assert_eq!(invoke_i32(&mut vm, "fac-i32", 8), 40_320);
}

#[test]
fn upstream_i32_fibonacci_recurses_through_table_slot() {
    let mut vm = instance();
    assert_eq!(invoke_i32(&mut vm, "fib-i32", 0), 1);
    assert_eq!(invoke_i32(&mut vm, "fib-i32", 1), 1);
    assert_eq!(invoke_i32(&mut vm, "fib-i32", 2), 2);
    assert_eq!(invoke_i32(&mut vm, "fib-i32", 5), 8);
    assert_eq!(invoke_i32(&mut vm, "fib-i32", 10), 89);
}
