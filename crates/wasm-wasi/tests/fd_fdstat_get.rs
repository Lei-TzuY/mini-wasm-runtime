use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, Value};
use wasm_wasi::{
    WasiPreview1, ERRNO_BADF, ERRNO_FAULT, ERRNO_SUCCESS, FILETYPE_CHARACTER_DEVICE,
    FILETYPE_DIRECTORY, RIGHTS_FD_READ, RIGHTS_FD_WRITE, RIGHTS_PATH_OPEN,
};

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
    let mut value = value as i32;
    loop {
        let mut byte = (value as u8) & 0x7f;
        let sign_bit_set = byte & 0x40 != 0;
        value >>= 7;
        let done = (value == 0 && !sign_bit_set) || (value == -1 && sign_bit_set);
        if !done {
            byte |= 0x80;
        }
        out.push(byte);
        if done {
            break;
        }
    }
}

fn module(fd: u32, fdstat: u32) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();

    let types = [2, 0x60, 2, 0x7f, 0x7f, 1, 0x7f, 0x60, 0, 1, 0x7f];
    section(&mut module, 1, &types);

    let mut imports = vec![2];
    name(&mut imports, "wasi_snapshot_preview1");
    name(&mut imports, "fd_fdstat_get");
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
    i32_const(&mut body, fdstat);
    body.extend([0x10, 0, 0x0b]);
    let mut code = vec![1];
    u32leb(&mut code, body.len() as u32);
    code.extend(body);
    section(&mut module, 10, &code);
    module
}

fn instantiate(fd: u32, fdstat: u32, memory: &MemoryHandle, wasi: &WasiPreview1) -> Instance {
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .unwrap();
    wasi.register(&mut hosts).unwrap();
    Instance::with_hosts(parse_module(&module(fd, fdstat)).unwrap(), hosts).unwrap()
}

#[test]
fn fd_fdstat_get_reports_bounded_stdin_metadata() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let wasi = WasiPreview1::new();
    let mut vm = instantiate(0, 32, &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );

    let fdstat = memory.read(32, 24).unwrap();
    assert_eq!(fdstat[0], FILETYPE_CHARACTER_DEVICE);
    assert_eq!(u16::from_le_bytes(fdstat[2..4].try_into().unwrap()), 0);
    assert_eq!(
        u64::from_le_bytes(fdstat[8..16].try_into().unwrap()),
        RIGHTS_FD_READ
    );
    assert_eq!(u64::from_le_bytes(fdstat[16..24].try_into().unwrap()), 0);
}

#[test]
fn fd_fdstat_get_reports_bounded_stdout_metadata() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let wasi = WasiPreview1::new();
    let mut vm = instantiate(1, 32, &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );

    let fdstat = memory.read(32, 24).unwrap();
    assert_eq!(fdstat[0], FILETYPE_CHARACTER_DEVICE);
    assert_eq!(u16::from_le_bytes(fdstat[2..4].try_into().unwrap()), 0);
    assert_eq!(
        u64::from_le_bytes(fdstat[8..16].try_into().unwrap()),
        RIGHTS_FD_WRITE
    );
    assert_eq!(u64::from_le_bytes(fdstat[16..24].try_into().unwrap()), 0);
}

#[test]
fn fd_fdstat_get_supports_stderr() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let wasi = WasiPreview1::new();
    let mut vm = instantiate(2, 64, &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    assert_eq!(memory.read(64, 1).unwrap(), [FILETYPE_CHARACTER_DEVICE]);
}

#[test]
fn fd_fdstat_get_reports_preopen_directory_path_rights() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let wasi = WasiPreview1::new().with_preopen("/sandbox").unwrap();
    let mut vm = instantiate(3, 128, &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    let fdstat = memory.read(128, 24).unwrap();
    assert_eq!(fdstat[0], FILETYPE_DIRECTORY);
    assert_eq!(u16::from_le_bytes(fdstat[2..4].try_into().unwrap()), 0);
    assert_eq!(
        u64::from_le_bytes(fdstat[8..16].try_into().unwrap()),
        RIGHTS_PATH_OPEN
    );
    assert_eq!(
        u64::from_le_bytes(fdstat[16..24].try_into().unwrap()),
        RIGHTS_FD_READ
    );
}

#[test]
fn fd_fdstat_get_bad_fd_does_not_mutate_guest_memory() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(96, &[0xaa; 24]).unwrap();
    let wasi = WasiPreview1::new();
    let mut vm = instantiate(9, 96, &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_BADF))
    );
    assert_eq!(memory.read(96, 24).unwrap(), vec![0xaa; 24]);
}

#[test]
fn fd_fdstat_get_oob_pointer_fails_without_partial_write() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(65_520, &[0xaa; 16]).unwrap();
    let wasi = WasiPreview1::new();
    let mut vm = instantiate(1, 65_520, &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_FAULT))
    );
    assert_eq!(memory.read(65_520, 16).unwrap(), vec![0xaa; 16]);
}
