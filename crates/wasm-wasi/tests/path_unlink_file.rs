use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, Value};
use wasm_wasi::{
    WasiPreview1, ERRNO_BADF, ERRNO_FAULT, ERRNO_INVAL, ERRNO_NAMETOOLONG, ERRNO_NOENT,
    ERRNO_NOTCAPABLE, ERRNO_SUCCESS, OFLAGS_CREAT, RIGHTS_FD_FILESTAT_GET, RIGHTS_FD_READ,
    RIGHTS_FD_SEEK, RIGHTS_FD_WRITE, RIGHTS_PATH_UNLINK_FILE,
};
const FILESTAT_SIZE: usize = 64;

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

    let mut types = vec![4];
    function_type(
        &mut types,
        &[0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7e, 0x7e, 0x7f, 0x7f],
    );
    function_type(&mut types, &[0x7f, 0x7f, 0x7f]);
    function_type(&mut types, &[0x7f, 0x7f]);
    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7e, 0x7f]);
    section(&mut module, 1, &types);

    let mut imports = vec![7];
    add_function_import(&mut imports, "path_open", 0);
    add_function_import(&mut imports, "path_unlink_file", 1);
    add_function_import(&mut imports, "fd_fdstat_get", 2);
    add_function_import(&mut imports, "fd_filestat_get", 2);
    add_function_import(&mut imports, "fd_pwrite", 3);
    add_function_import(&mut imports, "fd_pread", 3);
    name(&mut imports, "env");
    name(&mut imports, "memory");
    imports.extend([2, 0, 1]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[6, 0, 1, 2, 2, 3, 3]);

    let mut exports = vec![6];
    for (export_name, function_index) in [
        ("open", 6),
        ("unlink", 7),
        ("fdstat", 8),
        ("filestat", 9),
        ("pwrite", 10),
        ("pread", 11),
    ] {
        name(&mut exports, export_name);
        exports.push(0);
        u32leb(&mut exports, function_index);
    }
    section(&mut module, 7, &exports);

    let bodies = [
        forwarder(9, 0),
        forwarder(3, 1),
        forwarder(2, 2),
        forwarder(2, 3),
        forwarder(5, 4),
        forwarder(5, 5),
    ];
    let mut code = vec![6];
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

fn open_args(
    dir_fd: u32,
    path_ptr: i32,
    path_len: i32,
    open_flags: u32,
    rights: u64,
    opened_fd_ptr: i32,
) -> Vec<Value> {
    vec![
        Value::I32(dir_fd as i32),
        Value::I32(0),
        Value::I32(path_ptr),
        Value::I32(path_len),
        Value::I32(open_flags as i32),
        Value::I64(rights as i64),
        Value::I64(0),
        Value::I32(0),
        Value::I32(opened_fd_ptr),
    ]
}

fn unlink_args(dir_fd: u32, path_ptr: i32, path_len: i32) -> [Value; 3] {
    [
        Value::I32(dir_fd as i32),
        Value::I32(path_ptr),
        Value::I32(path_len),
    ]
}

fn read_u32(memory: &MemoryHandle, ptr: u32) -> u32 {
    u32::from_le_bytes(memory.read(ptr, 4).unwrap().try_into().unwrap())
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn filestat(memory: &MemoryHandle, ptr: u32) -> Vec<u8> {
    memory.read(ptr, FILESTAT_SIZE).unwrap()
}

#[test]
fn unlink_detaches_namespace_but_preserves_open_descriptor_lifetime() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"data.bin").unwrap();
    memory.write(128, &256u32.to_le_bytes()).unwrap();
    memory.write(132, &1u32.to_le_bytes()).unwrap();
    memory.write(256, b"Z").unwrap();
    memory.write(176, &300u32.to_le_bytes()).unwrap();
    memory.write(180, &3u32.to_le_bytes()).unwrap();

    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap()
        .with_writable_file("/sandbox", "data.bin", b"abc")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);
    let rights = RIGHTS_FD_READ | RIGHTS_FD_WRITE | RIGHTS_FD_SEEK | RIGHTS_FD_FILESTAT_GET;

    assert_eq!(
        errno(&mut vm, "fdstat", &[Value::I32(3), Value::I32(220)]),
        ERRNO_SUCCESS
    );
    let preopen_fdstat = memory.read(220, 24).unwrap();
    assert_ne!(read_u64(&preopen_fdstat, 8) & RIGHTS_PATH_UNLINK_FILE, 0);

    assert_eq!(
        errno(&mut vm, "open", &open_args(3, 64, 8, 0, rights, 100)),
        ERRNO_SUCCESS
    );
    let fd = read_u32(&memory, 100);
    assert_eq!(
        errno(
            &mut vm,
            "filestat",
            &[Value::I32(fd as i32), Value::I32(192)]
        ),
        ERRNO_SUCCESS
    );
    let before = filestat(&memory, 192);
    let inode = read_u64(&before, 8);
    assert_ne!(inode, 0);
    assert_eq!(read_u64(&before, 24), 1);
    assert_eq!(read_u64(&before, 32), 3);

    assert_eq!(
        errno(&mut vm, "unlink", &unlink_args(3, 64, 8)),
        ERRNO_SUCCESS
    );
    assert!(wasi.file_snapshot("/sandbox", "data.bin").is_none());

    memory.write(104, &0xfeedfaceu32.to_le_bytes()).unwrap();
    assert_eq!(
        errno(&mut vm, "open", &open_args(3, 64, 8, 0, rights, 104)),
        ERRNO_NOENT
    );
    assert_eq!(read_u32(&memory, 104), 0xfeedface);

    assert_eq!(
        errno(
            &mut vm,
            "filestat",
            &[Value::I32(fd as i32), Value::I32(192)]
        ),
        ERRNO_SUCCESS
    );
    let after = filestat(&memory, 192);
    assert_eq!(read_u64(&after, 8), inode);
    assert_eq!(read_u64(&after, 24), 0);
    assert_eq!(read_u64(&after, 32), 3);

    assert_eq!(
        errno(
            &mut vm,
            "pwrite",
            &[
                Value::I32(fd as i32),
                Value::I32(128),
                Value::I32(1),
                Value::I64(1),
                Value::I32(160),
            ]
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u32(&memory, 160), 1);
    assert_eq!(
        errno(
            &mut vm,
            "pread",
            &[
                Value::I32(fd as i32),
                Value::I32(176),
                Value::I32(1),
                Value::I64(0),
                Value::I32(184),
            ]
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u32(&memory, 184), 3);
    assert_eq!(memory.read(300, 3).unwrap(), b"aZc");

    assert_eq!(
        errno(&mut vm, "unlink", &unlink_args(3, 64, 8)),
        ERRNO_NOENT
    );
}

#[test]
fn unlink_enforces_directory_capability_and_fails_closed_on_bad_paths() {
    let ro_memory = MemoryHandle::new(1, Some(1)).unwrap();
    ro_memory.write(64, b"data.bin").unwrap();
    let readonly = WasiPreview1::new()
        .with_preopen("/ro")
        .unwrap()
        .with_read_only_file("/ro", "data.bin", b"abc")
        .unwrap();
    let mut ro_vm = instantiate(&ro_memory, &readonly);

    assert_eq!(
        errno(&mut ro_vm, "unlink", &unlink_args(3, 64, 8)),
        ERRNO_NOTCAPABLE
    );
    assert_eq!(readonly.file_snapshot("/ro", "data.bin").unwrap(), b"abc");
    assert_eq!(
        errno(&mut ro_vm, "unlink", &unlink_args(99, 64, 8)),
        ERRNO_BADF
    );

    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"data.bin").unwrap();
    memory.write(96, b"../x").unwrap();
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap()
        .with_read_only_file("/sandbox", "data.bin", b"abc")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);

    assert_eq!(
        errno(&mut vm, "unlink", &unlink_args(3, 64, 0)),
        ERRNO_INVAL
    );
    assert_eq!(wasi.file_snapshot("/sandbox", "data.bin").unwrap(), b"abc");
    assert_eq!(
        errno(&mut vm, "unlink", &unlink_args(3, 96, 4)),
        ERRNO_NOTCAPABLE
    );
    assert_eq!(wasi.file_snapshot("/sandbox", "data.bin").unwrap(), b"abc");
    assert_eq!(
        errno(&mut vm, "unlink", &unlink_args(3, 0, 4097)),
        ERRNO_NAMETOOLONG
    );
    assert_eq!(wasi.file_snapshot("/sandbox", "data.bin").unwrap(), b"abc");
    assert_eq!(
        errno(&mut vm, "unlink", &unlink_args(3, 65_532, 8)),
        ERRNO_FAULT
    );
    assert_eq!(wasi.file_snapshot("/sandbox", "data.bin").unwrap(), b"abc");

    assert_eq!(
        errno(&mut vm, "unlink", &unlink_args(3, 64, 8)),
        ERRNO_SUCCESS
    );
    assert!(wasi.file_snapshot("/sandbox", "data.bin").is_none());
}

#[test]
fn created_file_can_be_unlinked_while_its_descriptor_remains_live() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"new.bin").unwrap();
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);
    let rights = RIGHTS_FD_READ | RIGHTS_FD_WRITE | RIGHTS_FD_SEEK | RIGHTS_FD_FILESTAT_GET;

    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(3, 64, 7, OFLAGS_CREAT, rights, 100)
        ),
        ERRNO_SUCCESS
    );
    let fd = read_u32(&memory, 100);
    assert!(wasi.file_snapshot("/sandbox", "new.bin").is_some());

    assert_eq!(
        errno(&mut vm, "unlink", &unlink_args(3, 64, 7)),
        ERRNO_SUCCESS
    );
    assert!(wasi.file_snapshot("/sandbox", "new.bin").is_none());
    assert_eq!(
        errno(
            &mut vm,
            "filestat",
            &[Value::I32(fd as i32), Value::I32(192)]
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u64(&filestat(&memory, 192), 24), 0);
}
