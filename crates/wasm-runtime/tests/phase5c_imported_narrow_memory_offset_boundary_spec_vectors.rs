use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, RuntimeError, Value, WASM_PAGE_SIZE};

const I32: u8 = 0x7f;

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

fn push_export(payload: &mut Vec<u8>, name: &str, function_index: u32) {
    push_name(payload, name);
    payload.push(0x00);
    push_u32(payload, function_index);
}

fn push_body(payload: &mut Vec<u8>, instructions: &[u8]) {
    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    push_u32(payload, body.len() as u32);
    payload.extend_from_slice(&body);
}

fn imported_narrow_module(offset: u32) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(
        &mut module,
        1,
        &[
            0x02, // two types
            0x60, 0x01, I32, 0x01, I32, // (i32) -> i32
            0x60, 0x02, I32, I32, 0x00, // (i32, i32) -> ()
        ],
    );

    let mut imports = vec![0x01];
    push_name(&mut imports, "env");
    push_name(&mut imports, "mem");
    imports.extend([0x02, 0x01, 0x01, 0x02]); // memory min=1 max=2
    push_section(&mut module, 2, &imports);
    push_section(&mut module, 3, &[0x04, 0x00, 0x00, 0x01, 0x01]);

    let mut exports = vec![0x04];
    for (name, index) in [
        ("load8", 0u32),
        ("load16", 1),
        ("store8", 2),
        ("store16", 3),
    ] {
        push_export(&mut exports, name, index);
    }
    push_section(&mut module, 7, &exports);

    let mut code = vec![0x04];
    let mut load8 = vec![0x20, 0x00, 0x2d, 0x00];
    push_u32(&mut load8, offset);
    push_body(&mut code, &load8);

    let mut load16 = vec![0x20, 0x00, 0x2f, 0x01];
    push_u32(&mut load16, offset);
    push_body(&mut code, &load16);

    let mut store8 = vec![0x20, 0x00, 0x20, 0x01, 0x3a, 0x00];
    push_u32(&mut store8, offset);
    push_body(&mut code, &store8);

    let mut store16 = vec![0x20, 0x00, 0x20, 0x01, 0x3b, 0x01];
    push_u32(&mut store16, offset);
    push_body(&mut code, &store16);

    push_section(&mut module, 10, &code);
    module
}

fn instance(offset: u32, memory: &MemoryHandle) -> Instance {
    let module = parse_module(&imported_narrow_module(offset)).expect("imported narrow vector parses");
    let mut hosts = HostRegistry::new();
    hosts.register_memory("env", "mem", memory.clone()).unwrap();
    Instance::with_hosts(module, hosts).expect("imported narrow vector instantiates")
}

fn assert_memory_oob(error: RuntimeError, expected_address: u64, expected_width: usize) {
    match error {
        RuntimeError::MemoryOutOfBounds { address, width } => {
            assert_eq!(address, expected_address);
            assert_eq!(width, expected_width);
        }
        other => panic!("expected precise memory out-of-bounds trap, got {other:?}"),
    }
}

#[test]
fn imported_narrow_offsets_read_the_last_legal_bytes_from_host_backing() {
    let memory = MemoryHandle::new(1, Some(2)).unwrap();
    memory.write((WASM_PAGE_SIZE - 2) as u32, &[0x34, 0x12]).unwrap();
    let mut vm = instance(7, &memory);

    assert_eq!(
        vm.invoke_export("load8", &[Value::I32((WASM_PAGE_SIZE - 8) as i32)])
            .unwrap(),
        Some(Value::I32(0x12))
    );
    assert_eq!(
        vm.invoke_export("load16", &[Value::I32((WASM_PAGE_SIZE - 9) as i32)])
            .unwrap(),
        Some(Value::I32(0x1234))
    );
}

#[test]
fn imported_narrow_offsets_trap_at_precise_effective_addresses() {
    let memory = MemoryHandle::new(1, Some(2)).unwrap();
    let mut vm = instance(7, &memory);

    let error = vm
        .invoke_export("load8", &[Value::I32((WASM_PAGE_SIZE - 7) as i32)])
        .expect_err("effective page end must trap for load8");
    assert_memory_oob(error, WASM_PAGE_SIZE as u64, 1);

    let error = vm
        .invoke_export("load16", &[Value::I32((WASM_PAGE_SIZE - 8) as i32)])
        .expect_err("last byte cannot start a two-byte load");
    assert_memory_oob(error, (WASM_PAGE_SIZE - 1) as u64, 2);
}

#[test]
fn failed_imported_narrow_stores_leave_host_backing_unchanged() {
    let memory = MemoryHandle::new(1, Some(2)).unwrap();
    memory.write((WASM_PAGE_SIZE - 4) as u32, b"KEEP").unwrap();
    let mut vm = instance(7, &memory);

    let error = vm
        .invoke_export(
            "store8",
            &[
                Value::I32((WASM_PAGE_SIZE - 7) as i32),
                Value::I32(0xaa),
            ],
        )
        .expect_err("OOB store8 must trap before host mutation");
    assert_memory_oob(error, WASM_PAGE_SIZE as u64, 1);
    assert_eq!(memory.read((WASM_PAGE_SIZE - 4) as u32, 4).unwrap(), b"KEEP");

    let error = vm
        .invoke_export(
            "store16",
            &[
                Value::I32((WASM_PAGE_SIZE - 8) as i32),
                Value::I32(0x1234),
            ],
        )
        .expect_err("OOB store16 must trap before host mutation");
    assert_memory_oob(error, (WASM_PAGE_SIZE - 1) as u64, 2);
    assert_eq!(memory.read((WASM_PAGE_SIZE - 4) as u32, 4).unwrap(), b"KEEP");
}

#[test]
fn imported_narrow_effective_addresses_do_not_wrap_at_u32() {
    for (offset, base, expected_address) in [
        (1, -1, 0x1_0000_0000u64),
        (u32::MAX, 1, 0x1_0000_0000u64),
    ] {
        let memory = MemoryHandle::new(1, Some(2)).unwrap();
        let mut vm = instance(offset, &memory);

        for (name, width) in [("load8", 1usize), ("load16", 2usize)] {
            let error = vm
                .invoke_export(name, &[Value::I32(base)])
                .expect_err("effective address must not wrap into host memory");
            assert_memory_oob(error, expected_address, width);
        }
    }
}
