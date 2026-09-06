use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, Value};
use wasm_wasi::{WasiPreview1, ERRNO_FAULT, ERRNO_INVAL, ERRNO_SUCCESS};

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

fn i32leb(out: &mut Vec<u8>, mut value: i32) {
    loop {
        let byte = (value as u8) & 0x7f;
        value >>= 7;
        let done = (value == 0 && byte & 0x40 == 0) || (value == -1 && byte & 0x40 != 0);
        out.push(if done { byte } else { byte | 0x80 });
        if done {
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

fn module(import_name: &str, first: u32, second: u32) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();

    let types = [2, 0x60, 2, 0x7f, 0x7f, 1, 0x7f, 0x60, 0, 1, 0x7f];
    section(&mut module, 1, &types);

    let mut imports = vec![2];
    name(&mut imports, "wasi_snapshot_preview1");
    name(&mut imports, import_name);
    imports.extend([0, 0]);
    name(&mut imports, "env");
    name(&mut imports, "memory");
    imports.extend([2, 0, 1]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[1, 1]);

    let mut exports = vec![1];
    name(&mut exports, "run");
    exports.extend([0, 1]);
    section(&mut module, 7, &exports);

    let mut body = vec![0, 0x41];
    i32leb(&mut body, first as i32);
    body.push(0x41);
    i32leb(&mut body, second as i32);
    body.extend([0x10, 0, 0x0b]);
    let mut code = vec![1];
    u32leb(&mut code, body.len() as u32);
    code.extend(body);
    section(&mut module, 10, &code);
    module
}

fn instantiate(
    import_name: &str,
    first: u32,
    second: u32,
    memory: &MemoryHandle,
    wasi: &WasiPreview1,
) -> Instance {
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .unwrap();
    wasi.register(&mut hosts).unwrap();
    Instance::with_hosts(
        parse_module(&module(import_name, first, second)).unwrap(),
        hosts,
    )
    .unwrap()
}

fn read_u32(memory: &MemoryHandle, address: u32) -> u32 {
    let bytes = memory.read(address, 4).unwrap();
    u32::from_le_bytes(bytes.try_into().unwrap())
}

#[test]
fn environ_sizes_get_reports_configured_count_and_payload_bytes() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let wasi = WasiPreview1::new().with_env([("MODE", "test"), ("EMPTY", "")]);
    let mut vm = instantiate("environ_sizes_get", 0, 4, &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    assert_eq!(read_u32(&memory, 0), 2);
    assert_eq!(read_u32(&memory, 4), 16);
}

#[test]
fn environ_get_writes_pointer_table_and_nul_terminated_entries() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let wasi = WasiPreview1::new().with_env([("MODE", "test"), ("EMPTY", "")]);
    let mut vm = instantiate("environ_get", 0, 32, &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    assert_eq!(read_u32(&memory, 0), 32);
    assert_eq!(read_u32(&memory, 4), 42);
    assert_eq!(memory.read(32, 16).unwrap(), b"MODE=test\0EMPTY=\0");
}

#[test]
fn environ_get_oob_payload_does_not_partially_write_pointer_table() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(0, &[0xaa; 8]).unwrap();
    let wasi = WasiPreview1::new().with_env([("MODE", "test"), ("EMPTY", "")]);
    let mut vm = instantiate("environ_get", 0, 65_528, &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_FAULT))
    );
    assert_eq!(memory.read(0, 8).unwrap(), vec![0xaa; 8]);
}

#[test]
fn environ_sizes_get_preflights_both_output_words() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(0, &[0xaa; 4]).unwrap();
    let wasi = WasiPreview1::new().with_env([("MODE", "test")]);
    let mut vm = instantiate("environ_sizes_get", 0, 65_534, &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_FAULT))
    );
    assert_eq!(memory.read(0, 4).unwrap(), vec![0xaa; 4]);
}

#[test]
fn environ_get_rejects_configured_limits_before_guest_memory_access() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let wasi = WasiPreview1::new()
        .with_env([("MODE", "test"), ("EMPTY", "")])
        .with_env_limits(1, 1024);
    let mut vm = instantiate("environ_get", 65_535, 65_535, &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_INVAL))
    );
}
