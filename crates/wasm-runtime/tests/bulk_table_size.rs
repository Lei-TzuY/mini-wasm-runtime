use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, TableHandle, Value};
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

fn name(out: &mut Vec<u8>, value: &str) {
    u32leb(out, value.len() as u32);
    out.extend_from_slice(value.as_bytes());
}

fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    u32leb(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn module(table_index: u32, minimum: u8) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 0, 1, 0x7f]);

    let mut imports = vec![1];
    name(&mut imports, "env");
    name(&mut imports, "tab");
    imports.extend([1, 0x70, 0, minimum]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[1, 0]);
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);

    let mut body = vec![0, 0xfc, 16];
    u32leb(&mut body, table_index);
    body.push(0x0b);
    let mut code = vec![1];
    u32leb(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    section(&mut module, 10, &code);
    module
}

fn hosts(table: &TableHandle) -> HostRegistry {
    let mut hosts = HostRegistry::new();
    hosts.register_table("env", "tab", table.clone()).unwrap();
    hosts
}

#[test]
fn reports_imported_table_size() {
    let table = TableHandle::new(4, Some(8)).unwrap();
    let mut vm = Instance::with_hosts(parse_module(&module(0, 4)).unwrap(), hosts(&table)).unwrap();
    assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(4)));
}

#[test]
fn reports_zero_length_table() {
    let table = TableHandle::new(0, Some(8)).unwrap();
    let mut vm = Instance::with_hosts(parse_module(&module(0, 0)).unwrap(), hosts(&table)).unwrap();
    assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(0)));
}

#[test]
fn rejects_nonzero_table_index() {
    let table = TableHandle::new(4, Some(8)).unwrap();
    assert!(matches!(
        Instance::with_hosts(parse_module(&module(1, 4)).unwrap(), hosts(&table)),
        Err(wasm_runtime::RuntimeError::Validation(
            ValidationError::TableIndexOutOfBounds { table_index: 1, .. }
        ))
    ));
}
