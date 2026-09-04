use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, RuntimeError, TableHandle};
use wasm_validator::ValidationError;

fn u32leb(out: &mut Vec<u8>, mut v: u32) {
    loop {
        let mut b = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            b |= 0x80;
        }
        out.push(b);
        if v == 0 {
            break;
        }
    }
}
fn name(out: &mut Vec<u8>, s: &str) {
    u32leb(out, s.len() as u32);
    out.extend_from_slice(s.as_bytes());
}
fn section(m: &mut Vec<u8>, id: u8, p: &[u8]) {
    m.push(id);
    u32leb(m, p.len() as u32);
    m.extend_from_slice(p);
}
fn module(body: &[u8]) -> Vec<u8> {
    let mut m = b"\0asm\x01\0\0\0".to_vec();
    section(&mut m, 1, &[1, 0x60, 0, 0]);
    let mut i = vec![1];
    name(&mut i, "env");
    name(&mut i, "tab");
    i.extend([1, 0x70, 0, 2]);
    section(&mut m, 2, &i);
    section(&mut m, 3, &[1, 0]);
    section(&mut m, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);
    section(&mut m, 9, &[1, 1, 0, 1, 0]);
    let mut c = vec![1, (body.len() + 1) as u8, 0];
    c.extend_from_slice(body);
    section(&mut m, 10, &c);
    m
}
fn hosts(table: &TableHandle) -> HostRegistry {
    let mut h = HostRegistry::new();
    h.register_table("env", "tab", table.clone()).unwrap();
    h
}

#[test]
fn table_init_populates_imported_table() {
    let body = [0x41, 0, 0x41, 0, 0x41, 1, 0xfc, 12, 0, 0, 0x0b];
    let table = TableHandle::new(2, Some(2)).unwrap();
    let mut vm =
        Instance::with_hosts(parse_module(&module(&body)).unwrap(), hosts(&table)).unwrap();
    vm.invoke_export("run", &[]).unwrap();
    assert!(table.get(0).unwrap().is_some());
    assert!(table.get(1).unwrap().is_none());
}
#[test]
fn elem_drop_traps_followup_nonempty_init_atomically() {
    let body = [0xfc, 13, 0, 0x41, 0, 0x41, 0, 0x41, 1, 0xfc, 12, 0, 0, 0x0b];
    let table = TableHandle::new(2, Some(2)).unwrap();
    let mut vm =
        Instance::with_hosts(parse_module(&module(&body)).unwrap(), hosts(&table)).unwrap();
    assert!(matches!(
        vm.invoke_export("run", &[]),
        Err(RuntimeError::ElementSegmentSourceOutOfBounds { .. })
    ));
    assert!(table.get(0).unwrap().is_none());
}
#[test]
fn source_oob_is_atomic() {
    let body = [0x41, 0, 0x41, 1, 0x41, 1, 0xfc, 12, 0, 0, 0x0b];
    let table = TableHandle::new(2, Some(2)).unwrap();
    let mut vm =
        Instance::with_hosts(parse_module(&module(&body)).unwrap(), hosts(&table)).unwrap();
    assert!(matches!(
        vm.invoke_export("run", &[]),
        Err(RuntimeError::ElementSegmentSourceOutOfBounds { .. })
    ));
    assert!(table.get(0).unwrap().is_none());
}
#[test]
fn destination_oob_is_atomic() {
    let body = [0x41, 2, 0x41, 0, 0x41, 1, 0xfc, 12, 0, 0, 0x0b];
    let table = TableHandle::new(2, Some(2)).unwrap();
    let mut vm =
        Instance::with_hosts(parse_module(&module(&body)).unwrap(), hosts(&table)).unwrap();
    assert!(matches!(
        vm.invoke_export("run", &[]),
        Err(RuntimeError::TableElementOutOfBounds(_))
    ));
    assert!(table.get(0).unwrap().is_none() && table.get(1).unwrap().is_none());
}
#[test]
fn validator_rejects_bad_element_index() {
    let body = [0x41, 0, 0x41, 0, 0x41, 0, 0xfc, 12, 1, 0, 0x0b];
    let table = TableHandle::new(2, Some(2)).unwrap();
    assert!(matches!(
        Instance::with_hosts(parse_module(&module(&body)).unwrap(), hosts(&table)),
        Err(RuntimeError::Validation(
            ValidationError::ElementIndexOutOfBounds {
                element_index: 1,
                ..
            }
        ))
    ));
}
#[test]
fn validator_rejects_bad_table_index() {
    let body = [0x41, 0, 0x41, 0, 0x41, 0, 0xfc, 12, 0, 1, 0x0b];
    let table = TableHandle::new(2, Some(2)).unwrap();
    assert!(matches!(
        Instance::with_hosts(parse_module(&module(&body)).unwrap(), hosts(&table)),
        Err(RuntimeError::Validation(
            ValidationError::TableIndexOutOfBounds { table_index: 1, .. }
        ))
    ));
}
