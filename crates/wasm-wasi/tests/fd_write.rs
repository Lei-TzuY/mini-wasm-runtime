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
    u32leb(out, value);
}

fn module(fd: u32, iovs: u32, iovs_len: u32, nwritten: u32) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();

    let types = [
        2, 0x60, 4, 0x7f, 0x7f, 0x7f, 0x7f, 1, 0x7f, 0x60, 0, 1, 0x7f,
    ];
    section(&mut module, 1, &types);

    let mut imports = vec![2];
    name(&mut imports, "wasi_snapshot_preview1");
    name(&mut imports, "fd_write");
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
    i32_const(&mut body, nwritten);
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
    nwritten: u32,
    memory: &MemoryHandle,
    wasi: &WasiPreview1,
) -> Instance {
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .unwrap();
    wasi.register(&mut hosts).unwrap();
    Instance::with_hosts(
        parse_module(&module(fd, iovs, iovs_len, nwritten)).unwrap(),
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
fn fd_write_scatter_gathers_and_updates_nwritten() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    write_iovec(&memory, 0, 32, 5);
    write_iovec(&memory, 8, 37, 6);
    memory.write(32, b"hello world").unwrap();

    let wasi = WasiPreview1::new();
    let stdout = wasi.stdout();
    let mut vm = instantiate(1, 0, 2, 16, &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    assert_eq!(stdout.snapshot(), b"hello world");
    assert_eq!(read_u32(&memory, 16), 11);
}

#[test]
fn fd_write_routes_stderr_separately() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    write_iovec(&memory, 0, 24, 4);
    memory.write(24, b"oops").unwrap();

    let wasi = WasiPreview1::new();
    let stdout = wasi.stdout();
    let stderr = wasi.stderr();
    let mut vm = instantiate(2, 0, 1, 8, &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    assert!(stdout.snapshot().is_empty());
    assert_eq!(stderr.snapshot(), b"oops");
    assert_eq!(read_u32(&memory, 8), 4);
}

#[test]
fn fd_write_bad_fd_has_no_side_effects() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    write_iovec(&memory, 0, 24, 4);
    memory.write(24, b"nope").unwrap();

    let wasi = WasiPreview1::new();
    let stdout = wasi.stdout();
    let mut vm = instantiate(9, 0, 1, 8, &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_BADF))
    );
    assert!(stdout.snapshot().is_empty());
    assert_eq!(read_u32(&memory, 8), 0);
}

#[test]
fn fd_write_oob_payload_has_no_partial_output() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    write_iovec(&memory, 0, 32, 2);
    write_iovec(&memory, 8, 65_535, 2);
    memory.write(32, b"ok").unwrap();

    let wasi = WasiPreview1::new();
    let stdout = wasi.stdout();
    let mut vm = instantiate(1, 0, 2, 16, &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_FAULT))
    );
    assert!(stdout.snapshot().is_empty());
    assert_eq!(read_u32(&memory, 16), 0);
}

#[test]
fn fd_write_rejects_excessive_iovecs_before_reading_guest_memory() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let wasi = WasiPreview1::new().with_limits(1, 1024);
    let stdout = wasi.stdout();
    let mut vm = instantiate(1, 65_535, 2, 8, &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_INVAL))
    );
    assert!(stdout.snapshot().is_empty());
    assert_eq!(read_u32(&memory, 8), 0);
}
