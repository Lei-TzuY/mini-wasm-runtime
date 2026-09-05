use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, RuntimeError, TableHandle};
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
fn module(body: &[u8]) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 0, 0]);
    let mut imports = vec![1];
    name(&mut imports, "env");
    name(&mut imports, "tab");
    imports.extend([1, 0x70, 0, 4]);
    section(&mut module, 2, &imports);
    section(&mut module, 3, &[1, 0]);
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);
    section(&mut module, 9, &[1, 1, 0, 1, 0]);
    let mut code = vec![1, (body.len() + 1) as u8, 0];
    code.extend_from_slice(body);
    section(&mut module, 10, &code);
    module
}
fn hosts(table: &TableHandle) -> HostRegistry {
    let mut hosts = HostRegistry::new();
    hosts.register_table("env", "tab", table.clone()).unwrap();
    hosts
}
fn init(destination: u8, body: &mut Vec<u8>) {
    body.extend([0x41, destination, 0x41, 0, 0x41, 1, 0xfc, 12, 0, 0]);
}
fn copy(destination: u8, source: u8, length: u8, body: &mut Vec<u8>) {
    body.extend([
        0x41,
        destination,
        0x41,
        source,
        0x41,
        length,
        0xfc,
        14,
        0,
        0,
    ]);
}
fn present(table: &TableHandle) -> Vec<bool> {
    (0..table.len())
        .map(|i| table.get(i).unwrap().is_some())
        .collect()
}

#[test]
fn forward_overlap_is_memmove_safe() {
    let mut body = Vec::new();
    init(0, &mut body);
    init(2, &mut body);
    copy(1, 0, 3, &mut body);
    body.push(0x0b);
    let table = TableHandle::new(4, Some(4)).unwrap();
    let mut vm =
        Instance::with_hosts(parse_module(&module(&body)).unwrap(), hosts(&table)).unwrap();
    vm.invoke_export("run", &[]).unwrap();
    assert_eq!(present(&table), vec![true, true, false, true]);
}
#[test]
fn backward_overlap_is_memmove_safe() {
    let mut body = Vec::new();
    init(1, &mut body);
    init(3, &mut body);
    copy(0, 1, 3, &mut body);
    body.push(0x0b);
    let table = TableHandle::new(4, Some(4)).unwrap();
    let mut vm =
        Instance::with_hosts(parse_module(&module(&body)).unwrap(), hosts(&table)).unwrap();
    vm.invoke_export("run", &[]).unwrap();
    assert_eq!(present(&table), vec![true, false, true, true]);
}
#[test]
fn destination_oob_traps_atomically() {
    let mut body = Vec::new();
    init(0, &mut body);
    copy(3, 0, 2, &mut body);
    body.push(0x0b);
    let table = TableHandle::new(4, Some(4)).unwrap();
    let mut vm =
        Instance::with_hosts(parse_module(&module(&body)).unwrap(), hosts(&table)).unwrap();
    assert!(matches!(
        vm.invoke_export("run", &[]),
        Err(RuntimeError::TableElementOutOfBounds(_))
    ));
    assert_eq!(present(&table), vec![true, false, false, false]);
}
#[test]
fn source_oob_traps_atomically() {
    let mut body = Vec::new();
    init(0, &mut body);
    copy(1, 3, 2, &mut body);
    body.push(0x0b);
    let table = TableHandle::new(4, Some(4)).unwrap();
    let mut vm =
        Instance::with_hosts(parse_module(&module(&body)).unwrap(), hosts(&table)).unwrap();
    assert!(matches!(
        vm.invoke_export("run", &[]),
        Err(RuntimeError::TableElementOutOfBounds(_))
    ));
    assert_eq!(present(&table), vec![true, false, false, false]);
}
#[test]
fn rejects_nonzero_destination_table() {
    let body = [0x41, 0, 0x41, 0, 0x41, 0, 0xfc, 14, 1, 0, 0x0b];
    let table = TableHandle::new(4, Some(4)).unwrap();
    assert!(matches!(
        Instance::with_hosts(parse_module(&module(&body)).unwrap(), hosts(&table)),
        Err(RuntimeError::Validation(
            ValidationError::TableIndexOutOfBounds { table_index: 1, .. }
        ))
    ));
}
#[test]
fn rejects_nonzero_source_table() {
    let body = [0x41, 0, 0x41, 0, 0x41, 0, 0xfc, 14, 0, 1, 0x0b];
    let table = TableHandle::new(4, Some(4)).unwrap();
    assert!(matches!(
        Instance::with_hosts(parse_module(&module(&body)).unwrap(), hosts(&table)),
        Err(RuntimeError::Validation(
            ValidationError::TableIndexOutOfBounds { table_index: 1, .. }
        ))
    ));
}
