use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, RuntimeError, Value};
use wasm_validator::ValidationError;

fn u32leb(out: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut b = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            b |= 0x80;
        }
        out.push(b);
        if value == 0 {
            break;
        }
    }
}
fn name(out: &mut Vec<u8>, s: &str) {
    u32leb(out, s.len() as u32);
    out.extend_from_slice(s.as_bytes());
}
fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    u32leb(module, payload.len() as u32);
    module.extend_from_slice(payload);
}
fn header() -> Vec<u8> {
    b"\0asm\x01\0\0\0".to_vec()
}

fn two_defined_size(index: u8) -> Vec<u8> {
    let mut m = header();
    section(&mut m, 1, &[1, 0x60, 0, 1, 0x7f]);
    section(&mut m, 3, &[1, 0]);
    section(&mut m, 5, &[2, 1, 1, 2, 1, 2, 3]);
    section(&mut m, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);
    section(&mut m, 10, &[1, 4, 0, 0x3f, index, 0x0b]);
    m
}

fn imported_defined_copy() -> Vec<u8> {
    let mut m = header();
    section(&mut m, 1, &[1, 0x60, 0, 0]);
    let mut imports = vec![1];
    name(&mut imports, "env");
    name(&mut imports, "mem");
    imports.extend([2, 0, 1]);
    section(&mut m, 2, &imports);
    section(&mut m, 3, &[1, 0]);
    section(&mut m, 5, &[1, 0, 1]);
    section(&mut m, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);
    section(
        &mut m,
        10,
        &[1, 12, 0, 0x41, 0, 0x41, 0, 0x41, 4, 0xfc, 10, 0, 1, 0x0b],
    );
    let mut data = vec![1, 2, 1, 0x41, 0, 0x0b, 4];
    data.extend_from_slice(b"wasm");
    section(&mut m, 11, &data);
    m
}

#[test]
fn memory_size_executes_against_nonzero_defined_memory() {
    let module = parse_module(&two_defined_size(1)).unwrap();
    let mut vm = Instance::new(module).unwrap();
    assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(2)));
}

#[test]
fn imported_memory_precedes_defined_memory_and_cross_copy_executes() {
    let module = parse_module(&imported_defined_copy()).unwrap();
    assert_eq!(module.memory_count(), 2);
    let memory = MemoryHandle::new(1, None).unwrap();
    let mut hosts = HostRegistry::new();
    hosts.register_memory("env", "mem", memory.clone()).unwrap();
    let mut vm = Instance::with_hosts(module, hosts).unwrap();
    assert_eq!(memory.read(0, 4).unwrap(), vec![0, 0, 0, 0]);
    vm.invoke_export("run", &[]).unwrap();
    assert_eq!(memory.read(0, 4).unwrap(), b"wasm");
}

#[test]
fn invalid_memory_index_remains_fail_closed() {
    let module = parse_module(&two_defined_size(2)).unwrap();
    assert!(matches!(
        Instance::new(module),
        Err(RuntimeError::Validation(
            ValidationError::MemoryIndexOutOfBounds {
                memory_index: 2,
                ..
            }
        ))
    ));
}
