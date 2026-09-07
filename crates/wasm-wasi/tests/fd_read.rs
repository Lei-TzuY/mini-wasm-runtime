use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, Value};
use wasm_wasi::{WasiPreview1, ERRNO_BADF, ERRNO_FAULT, ERRNO_INVAL, ERRNO_SUCCESS};

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

fn module(fd: u32, iovs: u32, iovs_len: u32, nread: u32) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();

    let types = [
        2, 0x60, 4, 0x7f, 0x7f, 0x7f, 0x7f, 1, 0x7f, 0x60, 0, 1, 0x7f,
    ];
    section(&mut module, 1, &types);

    let mut imports = vec![2];
    name(&mut imports, "wasi_snapshot_preview1");
    name(&mut imports, "fd_read");
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
    i32_const(&mut body, fd);
    i32_const(&mut body, iovs);
    i32_const(&mut body, iovs_len);
    i32_const(&mut body, nread);
    body.extend([0x10, 0, 0x0b]);
    let mut code = vec![1];
    u32leb(&mut code, body.len() as u32);
    code.extend(body);
    section(&mut module, 10, &code);
    module
}

fn instantiate(
    fd: u32,
    iovs: u32,
    iovs_len: u32,
    nread: u32,
    memory: &MemoryHandle,
    wasi: &WasiPreview1,
) -> Instance {
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .unwrap();
    wasi.register(&mut hosts).unwrap();
    Instance::with_hosts(
        parse_module(&module(fd, iovs, iovs_len, nread)).unwrap(),
        hosts,
    )
    .unwrap()
}

fn write_iovec(memory: &MemoryHandle, address: u32, pointer: u32, length: u32) {
    let mut bytes = Vec::with_capacity(8);
    bytes.extend(pointer.to_le_bytes());
    bytes.extend(length.to_le_bytes());
    memory.write(address, &bytes).unwrap();
}

fn read_u32(memory: &MemoryHandle, address: u32) -> u32 {
    let bytes = memory.read(address, 4).unwrap();
    u32::from_le_bytes(bytes.try_into().unwrap())
}

#[test]
fn fd_read_scatter_writes_stdin_and_updates_nread() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    write_iovec(&memory, 0, 32, 5);
    write_iovec(&memory, 8, 48, 6);

    let wasi = WasiPreview1::new().with_stdin(b"hello world");
    let mut vm = instantiate(0, 0, 2, 16, &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    assert_eq!(memory.read(32, 5).unwrap(), b"hello");
    assert_eq!(memory.read(48, 6).unwrap(), b" world");
    assert_eq!(read_u32(&memory, 16), 11);
}

#[test]
fn fd_read_consumes_sequentially_and_reports_eof() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    write_iovec(&memory, 0, 32, 3);

    let wasi = WasiPreview1::new().with_stdin(b"abcde");
    let mut vm = instantiate(0, 0, 1, 8, &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    assert_eq!(memory.read(32, 3).unwrap(), b"abc");
    assert_eq!(read_u32(&memory, 8), 3);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    assert_eq!(memory.read(32, 3).unwrap(), b"dec");
    assert_eq!(read_u32(&memory, 8), 2);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    assert_eq!(memory.read(32, 3).unwrap(), b"dec");
    assert_eq!(read_u32(&memory, 8), 0);
}

#[test]
fn fd_read_bad_fd_has_no_side_effects_or_input_consumption() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    write_iovec(&memory, 0, 32, 2);
    memory.write(32, &[0xaa; 2]).unwrap();
    memory.write(8, &[0xbb; 4]).unwrap();

    let wasi = WasiPreview1::new().with_stdin(b"ok");
    let mut bad = instantiate(9, 0, 1, 8, &memory, &wasi);
    assert_eq!(
        bad.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_BADF))
    );
    assert_eq!(memory.read(32, 2).unwrap(), vec![0xaa; 2]);
    assert_eq!(memory.read(8, 4).unwrap(), vec![0xbb; 4]);

    let valid_memory = MemoryHandle::new(1, Some(1)).unwrap();
    write_iovec(&valid_memory, 0, 32, 2);
    let mut valid = instantiate(0, 0, 1, 8, &valid_memory, &wasi);
    assert_eq!(
        valid.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    assert_eq!(valid_memory.read(32, 2).unwrap(), b"ok");
    assert_eq!(read_u32(&valid_memory, 8), 2);
}

#[test]
fn fd_read_oob_destination_is_atomic_and_does_not_consume_input() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    write_iovec(&memory, 0, 32, 2);
    write_iovec(&memory, 8, 65_535, 2);
    memory.write(32, &[0xaa; 2]).unwrap();
    memory.write(16, &[0xbb; 4]).unwrap();

    let wasi = WasiPreview1::new().with_stdin(b"abcd");
    let mut bad = instantiate(0, 0, 2, 16, &memory, &wasi);
    assert_eq!(
        bad.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_FAULT))
    );
    assert_eq!(memory.read(32, 2).unwrap(), vec![0xaa; 2]);
    assert_eq!(memory.read(16, 4).unwrap(), vec![0xbb; 4]);

    let valid_memory = MemoryHandle::new(1, Some(1)).unwrap();
    write_iovec(&valid_memory, 0, 32, 4);
    let mut valid = instantiate(0, 0, 1, 8, &valid_memory, &wasi);
    assert_eq!(
        valid.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    assert_eq!(valid_memory.read(32, 4).unwrap(), b"abcd");
    assert_eq!(read_u32(&valid_memory, 8), 4);
}

#[test]
fn fd_read_rejects_excessive_iovecs_before_guest_memory_access() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let wasi = WasiPreview1::new()
        .with_stdin(b"abcd")
        .with_read_limits(1, 1024);
    let mut vm = instantiate(0, 65_535, 2, 8, &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_INVAL))
    );
    assert_eq!(read_u32(&memory, 8), 0);
}

#[test]
fn fd_read_rejects_total_capacity_over_configured_limit_without_consumption() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    write_iovec(&memory, 0, 32, 4);
    memory.write(32, &[0xaa; 4]).unwrap();

    let wasi = WasiPreview1::new()
        .with_stdin(b"abcd")
        .with_read_limits(4, 3);
    let mut bad = instantiate(0, 0, 1, 8, &memory, &wasi);
    assert_eq!(
        bad.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_INVAL))
    );
    assert_eq!(memory.read(32, 4).unwrap(), vec![0xaa; 4]);

    let valid_memory = MemoryHandle::new(1, Some(1)).unwrap();
    write_iovec(&valid_memory, 0, 32, 3);
    let mut valid = instantiate(0, 0, 1, 8, &valid_memory, &wasi);
    assert_eq!(
        valid.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    assert_eq!(valid_memory.read(32, 3).unwrap(), b"abc");
    assert_eq!(read_u32(&valid_memory, 8), 3);
}
