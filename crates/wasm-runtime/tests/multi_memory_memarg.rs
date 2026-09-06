use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, RuntimeError, Value};
use wasm_validator::ValidationError;
fn leb(o: &mut Vec<u8>, mut v: u32) {
    loop {
        let mut b = (v & 127) as u8;
        v >>= 7;
        if v != 0 {
            b |= 128
        }
        o.push(b);
        if v == 0 {
            break;
        }
    }
}
fn sec(m: &mut Vec<u8>, id: u8, p: &[u8]) {
    m.push(id);
    leb(m, p.len() as u32);
    m.extend_from_slice(p)
}
fn code(b: &[u8]) -> Vec<u8> {
    let mut p = vec![1];
    leb(&mut p, (b.len() + 1) as u32);
    p.push(0);
    p.extend_from_slice(b);
    p
}
fn two(b: &[u8], params: &[u8]) -> Vec<u8> {
    let mut m = b"\0asm\x01\0\0\0".to_vec();
    let mut t = vec![1, 0x60, params.len() as u8];
    t.extend_from_slice(params);
    t.extend([1, 0x7f]);
    sec(&mut m, 1, &t);
    sec(&mut m, 3, &[1, 0]);
    sec(&mut m, 5, &[2, 0, 1, 0, 1]);
    sec(&mut m, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);
    sec(&mut m, 10, &code(b));
    m
}
fn imp(b: &[u8]) -> Vec<u8> {
    let mut m = b"\0asm\x01\0\0\0".to_vec();
    sec(&mut m, 1, &[1, 0x60, 0, 1, 0x7f]);
    let i = vec![1, 3, b'e', b'n', b'v', 3, b'm', b'e', b'm', 2, 0, 1];
    sec(&mut m, 2, &i);
    sec(&mut m, 3, &[1, 0]);
    sec(&mut m, 5, &[1, 0, 1]);
    sec(&mut m, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);
    sec(&mut m, 10, &code(b));
    m
}
#[test]
fn second_memory_load_store() {
    let b = [
        0x41, 0, 0x41, 11, 0x36, 2, 0, 0x41, 0, 0x41, 42, 0x36, 0x42, 1, 0, 0x41, 0, 0x28, 0x42, 1,
        0, 0x0b,
    ];
    let mut v = Instance::new(parse_module(&two(&b, &[])).unwrap()).unwrap();
    assert_eq!(v.invoke_export("run", &[]).unwrap(), Some(Value::I32(42)));
    assert_eq!(&v.memory().unwrap().bytes()[..4], &11i32.to_le_bytes())
}
#[test]
fn legacy_memarg_memory_zero() {
    let b = [0x41, 0, 0x41, 21, 0x36, 2, 0, 0x41, 0, 0x28, 2, 0, 0x0b];
    let mut v = Instance::new(parse_module(&two(&b, &[])).unwrap()).unwrap();
    assert_eq!(v.invoke_export("run", &[]).unwrap(), Some(Value::I32(21)))
}
#[test]
fn imported_precedes_defined() {
    let b = [
        0x41, 0, 0x41, 42, 0x36, 0x42, 1, 0, 0x41, 0, 0x28, 0x42, 1, 0, 0x0b,
    ];
    let mem = MemoryHandle::new(1, None).unwrap();
    let mut h = HostRegistry::new();
    h.register_memory("env", "mem", mem.clone()).unwrap();
    let mut v = Instance::with_hosts(parse_module(&imp(&b)).unwrap(), h).unwrap();
    assert_eq!(v.invoke_export("run", &[]).unwrap(), Some(Value::I32(42)));
    assert_eq!(mem.read(0, 4).unwrap(), vec![0; 4])
}
#[test]
fn invalid_index_rejected() {
    let b = [0x41, 0, 0x28, 0x42, 2, 0, 0x0b];
    assert!(matches!(
        Instance::new(parse_module(&two(&b, &[])).unwrap()),
        Err(RuntimeError::Validation(
            ValidationError::MemoryIndexOutOfBounds {
                memory_index: 2,
                ..
            }
        ))
    ))
}
#[test]
fn indexed_oob_traps() {
    let b = [0x20, 0, 0x41, 42, 0x36, 0x42, 1, 0, 0x41, 0, 0x0b];
    let mut v = Instance::new(parse_module(&two(&b, &[0x7f])).unwrap()).unwrap();
    assert!(matches!(
        v.invoke_export("run", &[Value::I32(65535)]),
        Err(RuntimeError::MemoryOutOfBounds { width: 4, .. })
    ))
}
