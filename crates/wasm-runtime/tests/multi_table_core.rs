use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, RuntimeError, TableHandle, Value};
use wasm_validator::ValidationError;

fn u32leb(out: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    u32leb(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn defined_two_table_module(body: &[u8], with_target: bool) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 0, 1, 0x7f]);
    let function_count = if with_target { 2 } else { 1 };
    let mut functions = vec![function_count];
    functions.extend(std::iter::repeat(0).take(function_count as usize));
    section(&mut module, 3, &functions);
    section(&mut module, 4, &[2, 0x70, 1, 1, 1, 0x70, 1, 1, 1]);
    let run_index = if with_target { 1 } else { 0 };
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, run_index]);
    if with_target {
        section(&mut module, 9, &[1, 2, 1, 0x41, 0, 0x0b, 0, 1, 0]);
    }
    let mut code = vec![function_count];
    if with_target {
        code.extend([4, 0, 0x41, 42, 0x0b]);
    }
    code.push((body.len() + 1) as u8);
    code.push(0);
    code.extend_from_slice(body);
    section(&mut module, 10, &code);
    module
}

fn name(out: &mut Vec<u8>, value: &str) {
    u32leb(out, value.len() as u32);
    out.extend_from_slice(value.as_bytes());
}

fn imported_then_defined_module(body: &[u8]) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 0, 1, 0x7f]);
    let mut imports = vec![1];
    name(&mut imports, "env");
    name(&mut imports, "tab");
    imports.extend([1, 0x70, 1, 1, 1]);
    section(&mut module, 2, &imports);
    section(&mut module, 3, &[2, 0, 0]);
    section(&mut module, 4, &[1, 0x70, 1, 1, 1]);
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 1]);
    section(&mut module, 9, &[1, 2, 1, 0x41, 0, 0x0b, 0, 1, 0]);
    let mut code = vec![2, 4, 0, 0x41, 42, 0x0b, (body.len() + 1) as u8, 0];
    code.extend_from_slice(body);
    section(&mut module, 10, &code);
    module
}

#[test]
fn call_indirect_dispatches_through_second_defined_table() {
    let body = [0x41, 0, 0x11, 0, 1, 0x0b];
    let module = parse_module(&defined_two_table_module(&body, true)).unwrap();
    let mut instance = Instance::new(module).unwrap();
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(42))
    );
}

#[test]
fn table_get_and_set_execute_against_second_defined_table() {
    let body = [0x41, 0, 0xd0, 0x70, 0x26, 1, 0x41, 0, 0x25, 1, 0xd1, 0x0b];
    let module = parse_module(&defined_two_table_module(&body, false)).unwrap();
    let mut instance = Instance::new(module).unwrap();
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(1))
    );
}

#[test]
fn imported_table_precedes_defined_table_in_index_space() {
    let body = [0x41, 0, 0x11, 0, 1, 0x0b];
    let imported = TableHandle::new(1, Some(1)).unwrap();
    let mut hosts = HostRegistry::new();
    hosts.register_table("env", "tab", imported).unwrap();
    let module = parse_module(&imported_then_defined_module(&body)).unwrap();
    let mut instance = Instance::with_hosts(module, hosts).unwrap();
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(42))
    );
}

#[test]
fn invalid_third_table_index_stays_fail_closed() {
    let body = [0x41, 0, 0x25, 2, 0xd1, 0x0b];
    let module = parse_module(&defined_two_table_module(&body, false)).unwrap();
    assert!(matches!(
        Instance::new(module),
        Err(RuntimeError::Validation(
            ValidationError::TableIndexOutOfBounds { table_index: 2, .. }
        ))
    ));
}
