use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};

fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

fn two_table_module(body: &[u8], element_payload: Option<&[u8]>, table_payload: &[u8]) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 0, 1, 0x7f]);
    section(&mut module, 3, &[2, 0, 0]);
    section(&mut module, 4, table_payload);
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 1]);
    if let Some(element) = element_payload {
        section(&mut module, 9, element);
    }
    let mut code = vec![2, 4, 0, 0x41, 42, 0x0b, (body.len() + 1) as u8, 0];
    code.extend_from_slice(body);
    section(&mut module, 10, &code);
    module
}

#[test]
fn table_copy_crosses_distinct_tables() {
    let tables = [2, 0x70, 1, 1, 1, 0x70, 1, 1, 1];
    let element = [1, 2, 1, 0x41, 0, 0x0b, 0, 1, 0];
    let body = [
        0x41, 0, 0x41, 0, 0x41, 1, 0xfc, 14, 0, 1, 0x41, 0, 0x11, 0, 0, 0x0b,
    ];
    let module = parse_module(&two_table_module(&body, Some(&element), &tables)).unwrap();
    let mut instance = Instance::new(module).unwrap();
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(42))
    );
}

#[test]
fn table_grow_and_size_use_second_table() {
    let tables = [2, 0x70, 1, 1, 1, 0x70, 1, 1, 3];
    let body = [0xd0, 0x70, 0x41, 1, 0xfc, 15, 1, 0x1a, 0xfc, 16, 1, 0x0b];
    let module = parse_module(&two_table_module(&body, None, &tables)).unwrap();
    let mut instance = Instance::new(module).unwrap();
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(2))
    );
}

#[test]
fn table_fill_targets_second_table() {
    let tables = [2, 0x70, 1, 1, 1, 0x70, 1, 2, 2];
    let element = [1, 2, 1, 0x41, 0, 0x0b, 0, 2, 0, 0];
    let body = [
        0x41, 0, 0xd0, 0x70, 0x41, 2, 0xfc, 17, 1, 0x41, 1, 0x25, 1, 0xd1, 0x0b,
    ];
    let module = parse_module(&two_table_module(&body, Some(&element), &tables)).unwrap();
    let mut instance = Instance::new(module).unwrap();
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(1))
    );
}

#[test]
fn table_init_targets_second_table() {
    let tables = [2, 0x70, 1, 1, 1, 0x70, 1, 1, 1];
    let element = [1, 1, 0, 1, 0];
    let body = [
        0x41, 0, 0x41, 0, 0x41, 1, 0xfc, 12, 0, 1, 0x41, 0, 0x11, 0, 1, 0x0b,
    ];
    let module = parse_module(&two_table_module(&body, Some(&element), &tables)).unwrap();
    let mut instance = Instance::new(module).unwrap();
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(42))
    );
}
