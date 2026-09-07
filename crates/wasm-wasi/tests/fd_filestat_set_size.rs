use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, Value};
use wasm_wasi::{
    WasiPreview1, ERRNO_BADF, ERRNO_FBIG, ERRNO_NOTCAPABLE, ERRNO_SUCCESS, OFLAGS_CREAT,
    RIGHTS_FD_SEEK, RIGHTS_FD_TELL,
};

const RIGHTS_FD_FILESTAT_SET_SIZE: u64 = 1 << 22;
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

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
    function_type(&mut types, &[0x7f, 0x7e]);
    function_type(&mut types, &[0x7f, 0x7e, 0x7f, 0x7f]);
    function_type(&mut types, &[0x7f, 0x7f]);
    section(&mut module, 1, &types);

    let mut imports = vec![5];
    add_function_import(&mut imports, "path_open", 0);
    add_function_import(&mut imports, "fd_filestat_set_size", 1);
    add_function_import(&mut imports, "fd_seek", 2);
    add_function_import(&mut imports, "fd_tell", 3);
    name(&mut imports, "env");
    name(&mut imports, "memory");
    imports.extend([2, 0, 1]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[4, 0, 1, 2, 3]);

    let mut exports = vec![4];
    for (export_name, function_index) in [("open", 4), ("resize", 5), ("seek", 6), ("tell", 7)] {
        name(&mut exports, export_name);
        exports.push(0);
        u32leb(&mut exports, function_index);
    }
    section(&mut module, 7, &exports);

    let bodies = [
        forwarder(9, 0),
        forwarder(2, 1),
        forwarder(4, 2),
        forwarder(2, 3),
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

fn read_u32(memory: &MemoryHandle, ptr: u32) -> u32 {
    u32::from_le_bytes(memory.read(ptr, 4).unwrap().try_into().unwrap())
}

fn read_u64(memory: &MemoryHandle, ptr: u32) -> u64 {
    u64::from_le_bytes(memory.read(ptr, 8).unwrap().try_into().unwrap())
}

#[test]
fn fd_filestat_set_size_shrinks_extends_with_zero_fill_and_preserves_cursor() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"data.bin").unwrap();
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap()
        .with_writable_file("/sandbox", "data.bin", b"abcdef")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);
    let rights = RIGHTS_FD_FILESTAT_SET_SIZE | RIGHTS_FD_SEEK | RIGHTS_FD_TELL;

    assert_eq!(
        errno(&mut vm, "open", &open_args(64, 8, 0, rights, 100)),
        ERRNO_SUCCESS
    );
    let fd = read_u32(&memory, 100);
    assert_eq!(
        errno(
            &mut vm,
            "seek",
            &[
                Value::I32(fd as i32),
                Value::I64(4),
                Value::I32(0),
                Value::I32(120),
            ]
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u64(&memory, 120), 4);

    assert_eq!(
        errno(&mut vm, "resize", &[Value::I32(fd as i32), Value::I64(2)]),
        ERRNO_SUCCESS
    );
    assert_eq!(wasi.file_snapshot("/sandbox", "data.bin").unwrap(), b"ab");
    assert_eq!(
        errno(&mut vm, "tell", &[Value::I32(fd as i32), Value::I32(136)]),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u64(&memory, 136), 4);

    assert_eq!(
        errno(&mut vm, "resize", &[Value::I32(fd as i32), Value::I64(5)]),
        ERRNO_SUCCESS
    );
    assert_eq!(
        wasi.file_snapshot("/sandbox", "data.bin").unwrap(),
        vec![b'a', b'b', 0, 0, 0]
    );
    assert_eq!(
        errno(&mut vm, "tell", &[Value::I32(fd as i32), Value::I32(144)]),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u64(&memory, 144), 4);
}

#[test]
fn fd_filestat_set_size_enforces_rights_fd_class_and_size_limit() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"data.bin").unwrap();
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap()
        .with_writable_file("/sandbox", "data.bin", b"abc")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);

    assert_eq!(
        errno(&mut vm, "open", &open_args(64, 8, 0, RIGHTS_FD_SEEK, 100)),
        ERRNO_SUCCESS
    );
    let fd = read_u32(&memory, 100);
    assert_eq!(
        errno(&mut vm, "resize", &[Value::I32(fd as i32), Value::I64(1)]),
        ERRNO_NOTCAPABLE
    );
    assert_eq!(wasi.file_snapshot("/sandbox", "data.bin").unwrap(), b"abc");

    assert_eq!(
        errno(&mut vm, "resize", &[Value::I32(999), Value::I64(1)]),
        ERRNO_BADF
    );
    assert_eq!(
        errno(&mut vm, "resize", &[Value::I32(1), Value::I64(1)]),
        ERRNO_NOTCAPABLE
    );
    assert_eq!(
        errno(&mut vm, "resize", &[Value::I32(3), Value::I64(1)]),
        ERRNO_NOTCAPABLE
    );

    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"data.bin").unwrap();
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap()
        .with_writable_file("/sandbox", "data.bin", b"abc")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);
    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(64, 8, 0, RIGHTS_FD_FILESTAT_SET_SIZE, 100)
        ),
        ERRNO_SUCCESS
    );
    let fd = read_u32(&memory, 100);
    assert_eq!(
        errno(
            &mut vm,
            "resize",
            &[
                Value::I32(fd as i32),
                Value::I64((MAX_FILE_BYTES + 1) as i64),
            ]
        ),
        ERRNO_FBIG
    );
    assert_eq!(wasi.file_snapshot("/sandbox", "data.bin").unwrap(), b"abc");
}

#[test]
fn read_only_file_cannot_acquire_resize_right() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"data.bin").unwrap();
    let wasi = WasiPreview1::new()
        .with_preopen("/sandbox")
        .unwrap()
        .with_read_only_file("/sandbox", "data.bin", b"abc")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);

    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(64, 8, 0, RIGHTS_FD_FILESTAT_SET_SIZE, 100)
        ),
        ERRNO_NOTCAPABLE
    );
    assert_eq!(wasi.file_snapshot("/sandbox", "data.bin").unwrap(), b"abc");
}

#[test]
fn created_file_can_be_resized_with_zero_fill() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"new.bin").unwrap();
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);

    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(64, 7, OFLAGS_CREAT, RIGHTS_FD_FILESTAT_SET_SIZE, 100,)
        ),
        ERRNO_SUCCESS
    );
    let fd = read_u32(&memory, 100);
    assert_eq!(
        errno(&mut vm, "resize", &[Value::I32(fd as i32), Value::I64(4)]),
        ERRNO_SUCCESS
    );
    assert_eq!(
        wasi.file_snapshot("/sandbox", "new.bin").unwrap(),
        vec![0, 0, 0, 0]
    );
}
