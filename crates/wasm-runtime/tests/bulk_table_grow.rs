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

fn module(body: &[u8], table_index: u32, minimum: u8, maximum: u8) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 0, 1, 0x7f]);

    let mut imports = vec![1];
    name(&mut imports, "env");
    name(&mut imports, "tab");
    imports.extend([1, 0x70, 1, minimum, maximum]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[1, 0]);
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);

    let mut function_body = vec![0];
    function_body.extend_from_slice(body);
    function_body.extend([0xfc, 15]);
    u32leb(&mut function_body, table_index);
    function_body.push(0x0b);
    let mut code = vec![1];
    u32leb(&mut code, function_body.len() as u32);
    code.extend_from_slice(&function_body);
    section(&mut module, 10, &code);
    module
}

fn hosts(table: &TableHandle) -> HostRegistry {
    let mut hosts = HostRegistry::new();
    hosts.register_table("env", "tab", table.clone()).unwrap();
    hosts
}

#[test]
fn grows_imported_table_with_null_and_returns_previous_size() {
    let table = TableHandle::new(2, Some(4)).unwrap();
    let body = [0xd0, 0x70, 0x41, 0x02];
    let mut vm = Instance::with_hosts(
        parse_module(&module(&body, 0, 2, 4)).unwrap(),
        hosts(&table),
    )
    .unwrap();

    assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(2)));
    assert_eq!(table.len(), 4);
    assert!(table.get(2).unwrap().is_none());
    assert!(table.get(3).unwrap().is_none());
}

#[test]
fn grows_with_non_null_funcref_initializer() {
    let table = TableHandle::new(1, Some(3)).unwrap();
    let body = [0xd2, 0x00, 0x41, 0x01];
    let mut vm = Instance::with_hosts(
        parse_module(&module(&body, 0, 1, 3)).unwrap(),
        hosts(&table),
    )
    .unwrap();

    assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(1)));
    assert_eq!(table.len(), 2);
    assert!(table.get(1).unwrap().is_some());
}

#[test]
fn zero_delta_returns_size_without_mutation() {
    let table = TableHandle::new(2, Some(2)).unwrap();
    let body = [0xd0, 0x70, 0x41, 0x00];
    let mut vm = Instance::with_hosts(
        parse_module(&module(&body, 0, 2, 2)).unwrap(),
        hosts(&table),
    )
    .unwrap();

    assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(2)));
    assert_eq!(table.len(), 2);
}

#[test]
fn maximum_failure_returns_minus_one_and_is_atomic() {
    let table = TableHandle::new(2, Some(2)).unwrap();
    let body = [0xd2, 0x00, 0x41, 0x01];
    let mut vm = Instance::with_hosts(
        parse_module(&module(&body, 0, 2, 2)).unwrap(),
        hosts(&table),
    )
    .unwrap();

    assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(-1)));
    assert_eq!(table.len(), 2);
    assert!(table.get(0).unwrap().is_none());
    assert!(table.get(1).unwrap().is_none());
}

#[test]
fn validator_rejects_nonzero_table_index() {
    let table = TableHandle::new(1, Some(2)).unwrap();
    let body = [0xd0, 0x70, 0x41, 0x01];
    assert!(matches!(
        Instance::with_hosts(
            parse_module(&module(&body, 1, 1, 2)).unwrap(),
            hosts(&table)
        ),
        Err(wasm_runtime::RuntimeError::Validation(
            ValidationError::TableIndexOutOfBounds { table_index: 1, .. }
        ))
    ));
}

#[test]
fn validator_rejects_numeric_initializer() {
    let table = TableHandle::new(1, Some(2)).unwrap();
    let body = [0x41, 0x00, 0x41, 0x01];
    assert!(matches!(
        Instance::with_hosts(
            parse_module(&module(&body, 0, 1, 2)).unwrap(),
            hosts(&table)
        ),
        Err(wasm_runtime::RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}
