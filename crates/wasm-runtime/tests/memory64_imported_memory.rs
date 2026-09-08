use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, RuntimeError, Value};

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

fn imported_module(memory64: bool) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();

    // load(i64)->i32, store(i64,i32), size()->i64, grow(i64)->i64,
    // fill(i64,i32,i64). memory32 variants use i32 addresses/pages below.
    let address = if memory64 { 0x7e } else { 0x7f };
    let mut types = vec![5];
    types.extend_from_slice(&[0x60, 1, address, 1, 0x7f]);
    types.extend_from_slice(&[0x60, 2, address, 0x7f, 0]);
    types.extend_from_slice(&[0x60, 0, 1, address]);
    types.extend_from_slice(&[0x60, 1, address, 1, address]);
    types.extend_from_slice(&[0x60, 3, address, 0x7f, address, 0]);
    section(&mut module, 1, &types);

    let mut imports = vec![1];
    name(&mut imports, "env");
    name(&mut imports, "mem");
    imports.push(0x02);
    imports.extend_from_slice(if memory64 {
        &[0x05, 0x01, 0x02]
    } else {
        &[0x01, 0x01, 0x02]
    });
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[5, 0, 1, 2, 3, 4]);

    let mut exports = vec![5];
    for (export, index) in [
        ("load", 0),
        ("store", 1),
        ("size", 2),
        ("grow", 3),
        ("fill", 4),
    ] {
        name(&mut exports, export);
        exports.push(0);
        u32leb(&mut exports, index);
    }
    section(&mut module, 7, &exports);

    let bodies: [&[u8]; 5] = [
        &[0, 0x20, 0, 0x28, 2, 0, 0x0b],
        &[0, 0x20, 0, 0x20, 1, 0x36, 2, 0, 0x0b],
        &[0, 0x3f, 0, 0x0b],
        &[0, 0x20, 0, 0x40, 0, 0x0b],
        &[0, 0x20, 0, 0x20, 1, 0x20, 2, 0xfc, 0x0b, 0, 0x0b],
    ];
    let mut code = vec![bodies.len() as u8];
    for body in bodies {
        u32leb(&mut code, body.len() as u32);
        code.extend_from_slice(body);
    }
    section(&mut module, 10, &code);
    module
}

fn instantiate(memory: MemoryHandle, memory64: bool) -> Result<Instance, RuntimeError> {
    let module = parse_module(&imported_module(memory64)).expect("parse imported-memory fixture");
    let mut hosts = HostRegistry::new();
    hosts.register_memory("env", "mem", memory).unwrap();
    Instance::with_hosts(module, hosts)
}

#[test]
fn memory32_handle_is_rejected_as_an_address_width_mismatch_for_memory64_import() {
    let error = instantiate(MemoryHandle::new(1, Some(2)).unwrap(), true)
        .expect_err("memory32 host handle must not satisfy a memory64 import");
    assert!(matches!(
        error,
        RuntimeError::HostMemoryAddressWidthMismatch {
            expected_memory64: true,
            actual_memory64: false,
            ..
        }
    ));
}

#[test]
fn memory64_handle_is_rejected_by_memory32_import() {
    let error = instantiate(MemoryHandle::new64(1, Some(2)).unwrap(), false)
        .expect_err("memory64 host handle must not satisfy a memory32 import");
    assert!(matches!(
        error,
        RuntimeError::HostMemoryAddressWidthMismatch {
            expected_memory64: false,
            actual_memory64: true,
            ..
        }
    ));
}

#[test]
fn imported_memory64_executes_i64_addressed_scalar_and_shared_backing() {
    let memory = MemoryHandle::new64(1, Some(2)).unwrap();
    assert!(memory.is_memory64());
    memory.write(8, &123i32.to_le_bytes()).unwrap();
    let mut instance = instantiate(memory.clone(), true).expect("bind memory64 host handle");

    assert_eq!(
        instance.invoke_export("load", &[Value::I64(8)]).unwrap(),
        Some(Value::I32(123))
    );
    instance
        .invoke_export("store", &[Value::I64(12), Value::I32(77)])
        .unwrap();
    assert_eq!(memory.read(12, 4).unwrap(), 77i32.to_le_bytes());

    instance
        .invoke_export("fill", &[Value::I64(16), Value::I32(0xab), Value::I64(3)])
        .unwrap();
    assert_eq!(memory.read(16, 3).unwrap(), vec![0xab; 3]);
}

#[test]
fn imported_memory64_size_and_grow_are_i64_and_bidirectionally_visible() {
    let memory = MemoryHandle::new64(1, Some(2)).unwrap();
    let mut instance = instantiate(memory.clone(), true).unwrap();
    assert_eq!(
        instance.invoke_export("size", &[]).unwrap(),
        Some(Value::I64(1))
    );
    assert_eq!(
        instance.invoke_export("grow", &[Value::I64(1)]).unwrap(),
        Some(Value::I64(1))
    );
    assert_eq!(memory.size_pages(), 2);
    assert_eq!(
        instance.invoke_export("grow", &[Value::I64(1)]).unwrap(),
        Some(Value::I64(-1))
    );
}

#[test]
fn imported_memory64_dynamic_address_above_u32_is_not_truncated() {
    let memory = MemoryHandle::new64(1, Some(2)).unwrap();
    memory.write(0, &99i32.to_le_bytes()).unwrap();
    let mut instance = instantiate(memory, true).unwrap();
    let address = 1i64 << 32;
    let error = instance
        .invoke_export("load", &[Value::I64(address)])
        .expect_err("bounded memory64 must trap rather than truncate an i64 address");
    assert!(matches!(
        error,
        RuntimeError::MemoryOutOfBounds { address: actual, .. }
            if actual == address as u64
    ));
}

#[test]
fn imported_memory64_uses_full_width_limit_subtyping_before_runtime_cap() {
    for memory in [
        MemoryHandle::new64(0, Some(2)).unwrap(),
        MemoryHandle::new64(1, Some(3)).unwrap(),
    ] {
        assert!(matches!(
            instantiate(memory, true),
            Err(RuntimeError::HostMemoryLimitsMismatch { .. })
        ));
    }
}

#[test]
fn memory64_handle_preserves_the_existing_physical_page_ceiling() {
    let over_limit = u64::from(wasm_validator::MAX_MEMORY_PAGES) + 1;
    assert!(matches!(
        MemoryHandle::new64(1, Some(over_limit)),
        Err(wasm_runtime::MemoryHandleError::LimitExceeded { pages, .. }) if pages == over_limit
    ));
}
