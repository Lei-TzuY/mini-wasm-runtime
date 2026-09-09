use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, Value};
use wasm_wasi::{
    WasiPreview1, ERRNO_BADF, ERRNO_INVAL, ERRNO_NOTCAPABLE, ERRNO_SUCCESS, RIGHTS_FD_SEEK,
    RIGHTS_FD_WRITE,
};

const RIGHTS_FD_FDSTAT_SET_FLAGS: u64 = 1 << 3;
const FDFLAGS_APPEND: u32 = 1 << 0;

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

fn function_type(payload: &mut Vec<u8>, params: &[u8]) {
    payload.push(0x60);
    u32leb(payload, params.len() as u32);
    payload.extend_from_slice(params);
    payload.extend([1, 0x7f]);
}

fn add_function_import(imports: &mut Vec<u8>, function: &str, type_index: u32) {
    name(imports, "wasi_snapshot_preview1");
    name(imports, function);
    imports.push(0);
    u32leb(imports, type_index);
}

fn forwarder(param_count: u32, import_index: u32) -> Vec<u8> {
    let mut body = vec![0];
    for index in 0..param_count {
        body.push(0x20);
        u32leb(&mut body, index);
    }
    body.push(0x10);
    u32leb(&mut body, import_index);
    body.push(0x0b);
    body
}

fn module() -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();

    let mut types = vec![5];
    function_type(
        &mut types,
        &[0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7e, 0x7e, 0x7f, 0x7f],
    );
    function_type(&mut types, &[0x7f, 0x7f]);
    function_type(&mut types, &[0x7f, 0x7e, 0x7f, 0x7f]);
    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7f]);
    function_type(&mut types, &[0x7f, 0x7f]);
    section(&mut module, 1, &types);

    let mut imports = vec![6];
    add_function_import(&mut imports, "path_open", 0);
    add_function_import(&mut imports, "fd_fdstat_set_flags", 1);
    add_function_import(&mut imports, "fd_seek", 2);
    add_function_import(&mut imports, "fd_write", 3);
    add_function_import(&mut imports, "fd_fdstat_get", 4);
    name(&mut imports, "env");
    name(&mut imports, "memory");
    imports.extend([2, 0, 1]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[5, 0, 1, 2, 3, 4]);

    let mut exports = vec![5];
    for (export_name, function_index) in [
        ("open", 5),
        ("set_flags", 6),
        ("seek", 7),
        ("write", 8),
        ("fdstat", 9),
    ] {
        name(&mut exports, export_name);
        exports.push(0);
        u32leb(&mut exports, function_index);
    }
    section(&mut module, 7, &exports);

    let bodies = [
        forwarder(9, 0),
        forwarder(2, 1),
        forwarder(4, 2),
        forwarder(4, 3),
        forwarder(2, 4),
    ];
    let mut code = vec![5];
    for body in bodies {
        u32leb(&mut code, body.len() as u32);
        code.extend(body);
    }
    section(&mut module, 10, &code);
    module
}

fn instantiate(memory: &MemoryHandle, wasi: &WasiPreview1) -> Instance {
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .unwrap();
    wasi.register(&mut hosts).unwrap();
    Instance::with_hosts(parse_module(&module()).unwrap(), hosts).unwrap()
}

fn errno(vm: &mut Instance, export: &str, args: &[Value]) -> i32 {
    let values = vm.invoke_export_values(export, args).unwrap();
    let [Value::I32(errno)] = values.as_slice() else {
        panic!("WASI wrapper returned unexpected values: {values:?}");
    };
    *errno
}

fn open_args(rights: u64, fd_flags: u32) -> [Value; 9] {
    [
        Value::I32(3),
        Value::I32(0),
        Value::I32(64),
        Value::I32(8),
        Value::I32(0),
        Value::I64(rights as i64),
        Value::I64(0),
        Value::I32(fd_flags as i32),
        Value::I32(100),
    ]
}

fn seek_args(fd: u32, offset: i64) -> [Value; 4] {
    [
        Value::I32(fd as i32),
        Value::I64(offset),
        Value::I32(0),
        Value::I32(176),
    ]
}

fn write_args(fd: u32) -> [Value; 4] {
    [
        Value::I32(fd as i32),
        Value::I32(128),
        Value::I32(1),
        Value::I32(160),
    ]
}

fn setup_memory() -> MemoryHandle {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"seed.bin").unwrap();
    memory.write(128, &256u32.to_le_bytes()).unwrap();
    memory.write(132, &1u32.to_le_bytes()).unwrap();
    memory.write(256, b"Z").unwrap();
    memory
}

#[test]
fn append_flag_redirects_sequential_writes_to_eof_and_can_be_cleared() {
    let memory = setup_memory();
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/scratch")
        .unwrap()
        .with_writable_file("/scratch", "seed.bin", b"abc")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);
    let rights = RIGHTS_FD_WRITE | RIGHTS_FD_SEEK | RIGHTS_FD_FDSTAT_SET_FLAGS;

    assert_eq!(errno(&mut vm, "open", &open_args(rights, 0)), ERRNO_SUCCESS);
    let fd = u32::from_le_bytes(memory.read(100, 4).unwrap().try_into().unwrap());

    assert_eq!(
        errno(
            &mut vm,
            "set_flags",
            &[Value::I32(fd as i32), Value::I32(FDFLAGS_APPEND as i32)],
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(
            &mut vm,
            "fdstat",
            &[Value::I32(fd as i32), Value::I32(320)],
        ),
        ERRNO_SUCCESS
    );
    let fdstat = memory.read(320, 24).unwrap();
    assert_eq!(u16::from_le_bytes(fdstat[2..4].try_into().unwrap()), 1);

    assert_eq!(errno(&mut vm, "seek", &seek_args(fd, 0)), ERRNO_SUCCESS);
    assert_eq!(errno(&mut vm, "write", &write_args(fd)), ERRNO_SUCCESS);
    assert_eq!(wasi.file_snapshot("/scratch", "seed.bin").unwrap(), b"abcZ");

    assert_eq!(
        errno(
            &mut vm,
            "set_flags",
            &[Value::I32(fd as i32), Value::I32(0)],
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(errno(&mut vm, "seek", &seek_args(fd, 0)), ERRNO_SUCCESS);
    memory.write(256, b"Y").unwrap();
    assert_eq!(errno(&mut vm, "write", &write_args(fd)), ERRNO_SUCCESS);
    assert_eq!(wasi.file_snapshot("/scratch", "seed.bin").unwrap(), b"YbcZ");
}

#[test]
fn append_can_be_requested_at_path_open() {
    let memory = setup_memory();
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/scratch")
        .unwrap()
        .with_writable_file("/scratch", "seed.bin", b"abc")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);
    let rights = RIGHTS_FD_WRITE | RIGHTS_FD_SEEK | RIGHTS_FD_FDSTAT_SET_FLAGS;

    assert_eq!(
        errno(&mut vm, "open", &open_args(rights, FDFLAGS_APPEND)),
        ERRNO_SUCCESS
    );
    let fd = u32::from_le_bytes(memory.read(100, 4).unwrap().try_into().unwrap());
    assert_eq!(errno(&mut vm, "seek", &seek_args(fd, 0)), ERRNO_SUCCESS);
    assert_eq!(errno(&mut vm, "write", &write_args(fd)), ERRNO_SUCCESS);
    assert_eq!(wasi.file_snapshot("/scratch", "seed.bin").unwrap(), b"abcZ");
}

#[test]
fn set_flags_requires_capability_and_rejects_unsupported_flags_fail_closed() {
    let memory = setup_memory();
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/scratch")
        .unwrap()
        .with_writable_file("/scratch", "seed.bin", b"abc")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);

    assert_eq!(
        errno(&mut vm, "open", &open_args(RIGHTS_FD_WRITE | RIGHTS_FD_SEEK, 0)),
        ERRNO_SUCCESS
    );
    let fd = u32::from_le_bytes(memory.read(100, 4).unwrap().try_into().unwrap());
    assert_eq!(
        errno(
            &mut vm,
            "set_flags",
            &[Value::I32(fd as i32), Value::I32(FDFLAGS_APPEND as i32)],
        ),
        ERRNO_NOTCAPABLE
    );
    assert_eq!(wasi.file_snapshot("/scratch", "seed.bin").unwrap(), b"abc");

    assert_eq!(
        errno(
            &mut vm,
            "set_flags",
            &[Value::I32(999), Value::I32(FDFLAGS_APPEND as i32)],
        ),
        ERRNO_BADF
    );

    assert_eq!(
        errno(
            &mut vm,
            "set_flags",
            &[Value::I32(3), Value::I32(FDFLAGS_APPEND as i32)],
        ),
        ERRNO_NOTCAPABLE
    );

    let memory2 = setup_memory();
    let wasi2 = WasiPreview1::new()
        .with_writable_preopen("/scratch")
        .unwrap()
        .with_writable_file("/scratch", "seed.bin", b"abc")
        .unwrap();
    let mut vm2 = instantiate(&memory2, &wasi2);
    assert_eq!(
        errno(
            &mut vm2,
            "open",
            &open_args(
                RIGHTS_FD_WRITE | RIGHTS_FD_SEEK | RIGHTS_FD_FDSTAT_SET_FLAGS,
                0,
            ),
        ),
        ERRNO_SUCCESS
    );
    let fd2 = u32::from_le_bytes(memory2.read(100, 4).unwrap().try_into().unwrap());
    assert_eq!(
        errno(
            &mut vm2,
            "set_flags",
            &[Value::I32(fd2 as i32), Value::I32(1 << 1)],
        ),
        ERRNO_INVAL
    );
    assert_eq!(wasi2.file_snapshot("/scratch", "seed.bin").unwrap(), b"abc");
}
