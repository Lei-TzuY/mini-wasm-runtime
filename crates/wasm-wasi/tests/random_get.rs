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

fn i32_const(out: &mut Vec<u8>, value: u32) {
    out.push(0x41);
    i32leb(out, value as i32);
}

fn module(buffer: u32, buffer_len: u32) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();

    let types = [2, 0x60, 2, 0x7f, 0x7f, 1, 0x7f, 0x60, 0, 1, 0x7f];
    section(&mut module, 1, &types);

    let mut imports = vec![2];
    name(&mut imports, "wasi_snapshot_preview1");
    name(&mut imports, "random_get");
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

    let mut body = vec![0];
    i32_const(&mut body, buffer);
    i32_const(&mut body, buffer_len);
    body.extend([0x10, 0, 0x0b]);
    let mut code = vec![1];
    u32leb(&mut code, body.len() as u32);
    code.extend(body);
    section(&mut module, 10, &code);
    module
}

fn instantiate(
    buffer: u32,
    buffer_len: u32,
    memory: &MemoryHandle,
    wasi: &WasiPreview1,
) -> Instance {
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .unwrap();
    wasi.register(&mut hosts).unwrap();
    Instance::with_hosts(parse_module(&module(buffer, buffer_len)).unwrap(), hosts).unwrap()
}

#[test]
fn random_get_writes_injected_bytes_and_consumes_sequentially() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let wasi = WasiPreview1::new().with_random_bytes(b"abcdef");
    let mut vm = instantiate(32, 3, &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    assert_eq!(memory.read(32, 3).unwrap(), b"abc");

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    assert_eq!(memory.read(32, 3).unwrap(), b"def");
}

#[test]
fn random_get_without_injected_entropy_fails_closed() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(32, &[0xaa; 2]).unwrap();
    let wasi = WasiPreview1::new();
    let mut vm = instantiate(32, 2, &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_INVAL))
    );
    assert_eq!(memory.read(32, 2).unwrap(), vec![0xaa; 2]);
}

#[test]
fn random_get_rejects_configured_limit_without_consuming_entropy() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(32, &[0xaa; 4]).unwrap();
    let wasi = WasiPreview1::new()
        .with_random_bytes(b"abcd")
        .with_random_limit(3);
    let mut bad = instantiate(32, 4, &memory, &wasi);

    assert_eq!(
        bad.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_INVAL))
    );
    assert_eq!(memory.read(32, 4).unwrap(), vec![0xaa; 4]);

    let valid_memory = MemoryHandle::new(1, Some(1)).unwrap();
    let mut valid = instantiate(32, 3, &valid_memory, &wasi);
    assert_eq!(
        valid.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    assert_eq!(valid_memory.read(32, 3).unwrap(), b"abc");
}

#[test]
fn random_get_oob_destination_is_atomic_and_does_not_consume_entropy() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let wasi = WasiPreview1::new().with_random_bytes(b"abcd");
    let mut bad = instantiate(65_535, 2, &memory, &wasi);

    assert_eq!(
        bad.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_FAULT))
    );

    let valid_memory = MemoryHandle::new(1, Some(1)).unwrap();
    let mut valid = instantiate(32, 4, &valid_memory, &wasi);
    assert_eq!(
        valid.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    assert_eq!(valid_memory.read(32, 4).unwrap(), b"abcd");
}

#[test]
fn random_get_short_source_is_atomic_and_does_not_consume_entropy() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(32, &[0xaa; 4]).unwrap();
    let wasi = WasiPreview1::new().with_random_bytes(b"abc");
    let mut bad = instantiate(32, 4, &memory, &wasi);

    assert_eq!(
        bad.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_INVAL))
    );
    assert_eq!(memory.read(32, 4).unwrap(), vec![0xaa; 4]);

    let valid_memory = MemoryHandle::new(1, Some(1)).unwrap();
    let mut valid = instantiate(32, 3, &valid_memory, &wasi);
    assert_eq!(
        valid.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    assert_eq!(valid_memory.read(32, 3).unwrap(), b"abc");
}
