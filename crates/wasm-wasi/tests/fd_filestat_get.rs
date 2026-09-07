use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, Value};
use wasm_wasi::{
    WasiPreview1, ERRNO_BADF, ERRNO_FAULT, ERRNO_NOTCAPABLE, ERRNO_SUCCESS, FILETYPE_REGULAR_FILE,
    OFLAGS_CREAT, RIGHTS_FD_READ, RIGHTS_FD_SEEK, RIGHTS_FD_WRITE,
};

const RIGHTS_FD_FILESTAT_GET: u64 = 1 << 21;
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
    function_type(&mut types, &[0x7f, 0x7f]);
    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7e, 0x7f]);
    function_type(&mut types, &[0x7f]);
    section(&mut module, 1, &types);

    let mut imports = vec![5];
    add_function_import(&mut imports, "path_open", 0);
    add_function_import(&mut imports, "fd_filestat_get", 1);
    add_function_import(&mut imports, "fd_pwrite", 2);
    add_function_import(&mut imports, "fd_close", 3);
    name(&mut imports, "env");
    name(&mut imports, "memory");
    imports.extend([2, 0, 1]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[4, 0, 1, 2, 3]);

    let mut exports = vec![4];
    for (export_name, function_index) in [("open", 4), ("filestat", 5), ("pwrite", 6), ("close", 7)]
    {
        name(&mut exports, export_name);
        exports.push(0);
        u32leb(&mut exports, function_index);
    }
    section(&mut module, 7, &exports);

    let bodies = [
        forwarder(9, 0),
        forwarder(2, 1),
        forwarder(5, 2),
        forwarder(1, 3),
    ];
    let mut code = vec![4];
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
    path_ptr: i32,
    path_len: i32,
    open_flags: u32,
    rights: u64,
    opened_fd_ptr: i32,
) -> Vec<Value> {
    vec![
        Value::I32(3),
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

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn filestat(memory: &MemoryHandle, ptr: u32) -> Vec<u8> {
    memory.read(ptr, FILESTAT_SIZE).unwrap()
}

#[test]
fn fd_filestat_get_tracks_stable_identity_and_live_shared_size() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"data.bin").unwrap();
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap()
        .with_writable_file("/sandbox", "data.bin", b"abc")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);
    let rights = RIGHTS_FD_READ | RIGHTS_FD_WRITE | RIGHTS_FD_SEEK | RIGHTS_FD_FILESTAT_GET;

    assert_eq!(
        errno(&mut vm, "open", &open_args(64, 8, 0, rights, 100)),
        ERRNO_SUCCESS
    );
    let fd = u32::from_le_bytes(memory.read(100, 4).unwrap().try_into().unwrap());
    assert_eq!(
        errno(
            &mut vm,
            "filestat",
            &[Value::I32(fd as i32), Value::I32(192)]
        ),
        ERRNO_SUCCESS
    );
    let initial = filestat(&memory, 192);
    let dev = read_u64(&initial, 0);
    let ino = read_u64(&initial, 8);
    assert_ne!(dev, 0);
    assert_ne!(ino, 0);
    assert_eq!(initial[16], FILETYPE_REGULAR_FILE);
    assert_eq!(read_u64(&initial, 24), 1);
    assert_eq!(read_u64(&initial, 32), 3);
    assert_eq!(read_u64(&initial, 40), 0);
    assert_eq!(read_u64(&initial, 48), 0);
    assert_eq!(read_u64(&initial, 56), 0);

    memory.write(128, &256u32.to_le_bytes()).unwrap();
    memory.write(132, &2u32.to_le_bytes()).unwrap();
    memory.write(256, b"XY").unwrap();
    assert_eq!(
        errno(
            &mut vm,
            "pwrite",
            &[
                Value::I32(fd as i32),
                Value::I32(128),
                Value::I32(1),
                Value::I64(6),
                Value::I32(160),
            ]
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(
            &mut vm,
            "filestat",
            &[Value::I32(fd as i32), Value::I32(192)]
        ),
        ERRNO_SUCCESS
    );
    let grown = filestat(&memory, 192);
    assert_eq!(read_u64(&grown, 0), dev);
    assert_eq!(read_u64(&grown, 8), ino);
    assert_eq!(read_u64(&grown, 32), 8);

    assert_eq!(
        errno(&mut vm, "close", &[Value::I32(fd as i32)]),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(&mut vm, "open", &open_args(64, 8, 0, rights, 100)),
        ERRNO_SUCCESS
    );
    let reopened_fd = u32::from_le_bytes(memory.read(100, 4).unwrap().try_into().unwrap());
    assert_eq!(
        errno(
            &mut vm,
            "filestat",
            &[Value::I32(reopened_fd as i32), Value::I32(192)]
        ),
        ERRNO_SUCCESS
    );
    let reopened = filestat(&memory, 192);
    assert_eq!(read_u64(&reopened, 0), dev);
    assert_eq!(read_u64(&reopened, 8), ino);
    assert_eq!(read_u64(&reopened, 32), 8);
}

#[test]
fn fd_filestat_get_enforces_rights_and_preflights_guest_memory() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"data.bin").unwrap();
    let wasi = WasiPreview1::new()
        .with_preopen("/sandbox")
        .unwrap()
        .with_read_only_file("/sandbox", "data.bin", b"abc")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);

    assert_eq!(
        errno(&mut vm, "open", &open_args(64, 8, 0, RIGHTS_FD_READ, 100)),
        ERRNO_SUCCESS
    );
    let fd = u32::from_le_bytes(memory.read(100, 4).unwrap().try_into().unwrap());
    memory.write(192, &[0xa5; FILESTAT_SIZE]).unwrap();
    assert_eq!(
        errno(
            &mut vm,
            "filestat",
            &[Value::I32(fd as i32), Value::I32(192)]
        ),
        ERRNO_NOTCAPABLE
    );
    assert_eq!(filestat(&memory, 192), vec![0xa5; FILESTAT_SIZE]);

    assert_eq!(
        errno(&mut vm, "filestat", &[Value::I32(999), Value::I32(192)]),
        ERRNO_BADF
    );
    assert_eq!(filestat(&memory, 192), vec![0xa5; FILESTAT_SIZE]);

    assert_eq!(
        errno(&mut vm, "close", &[Value::I32(fd as i32)]),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(64, 8, 0, RIGHTS_FD_READ | RIGHTS_FD_FILESTAT_GET, 100)
        ),
        ERRNO_SUCCESS
    );
    let fd = u32::from_le_bytes(memory.read(100, 4).unwrap().try_into().unwrap());
    memory.write(65_504, &[0x5a; 32]).unwrap();
    assert_eq!(
        errno(
            &mut vm,
            "filestat",
            &[Value::I32(fd as i32), Value::I32(65_504)]
        ),
        ERRNO_FAULT
    );
    assert_eq!(memory.read(65_504, 32).unwrap(), vec![0x5a; 32]);
}

#[test]
fn created_file_keeps_inode_across_close_and_reopen() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"new.bin").unwrap();
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);
    let rights = RIGHTS_FD_WRITE | RIGHTS_FD_SEEK | RIGHTS_FD_FILESTAT_GET;

    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(64, 7, OFLAGS_CREAT, rights, 100)
        ),
        ERRNO_SUCCESS
    );
    let fd = u32::from_le_bytes(memory.read(100, 4).unwrap().try_into().unwrap());
    assert_eq!(
        errno(
            &mut vm,
            "filestat",
            &[Value::I32(fd as i32), Value::I32(192)]
        ),
        ERRNO_SUCCESS
    );
    let created = filestat(&memory, 192);
    let ino = read_u64(&created, 8);
    assert_ne!(ino, 0);
    assert_eq!(read_u64(&created, 32), 0);

    assert_eq!(
        errno(&mut vm, "close", &[Value::I32(fd as i32)]),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(&mut vm, "open", &open_args(64, 7, 0, rights, 100)),
        ERRNO_SUCCESS
    );
    let reopened_fd = u32::from_le_bytes(memory.read(100, 4).unwrap().try_into().unwrap());
    assert_eq!(
        errno(
            &mut vm,
            "filestat",
            &[Value::I32(reopened_fd as i32), Value::I32(192)]
        ),
        ERRNO_SUCCESS
    );
    let reopened = filestat(&memory, 192);
    assert_eq!(read_u64(&reopened, 8), ino);
    assert_eq!(read_u64(&reopened, 32), 0);
}
