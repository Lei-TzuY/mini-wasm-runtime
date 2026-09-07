use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, Value};
use wasm_wasi::{
    WasiPreview1, ERRNO_BADF, ERRNO_FAULT, ERRNO_INVAL, ERRNO_NOENT, ERRNO_NOTCAPABLE,
    ERRNO_SUCCESS, RIGHTS_FD_FILESTAT_GET, RIGHTS_FD_READ, RIGHTS_FD_SEEK,
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

    let mut types = vec![5];
    function_type(
        &mut types,
        &[0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7e, 0x7e, 0x7f, 0x7f],
    );
    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f]);
    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f]);
    function_type(&mut types, &[0x7f, 0x7f]);
    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7e, 0x7f]);
    section(&mut module, 1, &types);

    let mut imports = vec![6];
    add_function_import(&mut imports, "path_open", 0);
    add_function_import(&mut imports, "path_link", 1);
    add_function_import(&mut imports, "path_rename", 2);
    add_function_import(&mut imports, "fd_filestat_get", 3);
    add_function_import(&mut imports, "fd_pread", 4);
    name(&mut imports, "env");
    name(&mut imports, "memory");
    imports.extend([2, 0, 1]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[5, 0, 1, 2, 3, 4]);

    let mut exports = vec![5];
    for (export_name, function_index) in [
        ("open", 5),
        ("link", 6),
        ("rename", 7),
        ("filestat", 8),
        ("pread", 9),
    ] {
        name(&mut exports, export_name);
        exports.push(0);
        u32leb(&mut exports, function_index);
    }
    section(&mut module, 7, &exports);

    let bodies = [
        forwarder(9, 0),
        forwarder(7, 1),
        forwarder(6, 2),
        forwarder(2, 3),
        forwarder(5, 4),
    ];
    let mut code = vec![5];
    for body in bodies {
        u32leb(&mut code, body.len() as u32);
        code.extend(body);
    }
    section(&mut module, 10, &code);
    module
}

fn instantiate(
    memory: &MemoryHandle,
    wasi: &WasiPreview1,
) -> Result<Instance, wasm_runtime::RuntimeError> {
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .unwrap();
    wasi.register(&mut hosts).unwrap();
    Instance::with_hosts(parse_module(&module()).unwrap(), hosts)
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
    rights: u64,
    opened_fd_ptr: i32,
) -> Vec<Value> {
    vec![
        Value::I32(dir_fd as i32),
        Value::I32(0),
        Value::I32(path_ptr),
        Value::I32(path_len),
        Value::I32(0),
        Value::I64(rights as i64),
        Value::I64(0),
        Value::I32(0),
        Value::I32(opened_fd_ptr),
    ]
}

fn link_args(
    old_fd: u32,
    old_path_ptr: i32,
    old_path_len: i32,
    new_fd: u32,
    new_path_ptr: i32,
    new_path_len: i32,
) -> [Value; 7] {
    [
        Value::I32(old_fd as i32),
        Value::I32(0),
        Value::I32(old_path_ptr),
        Value::I32(old_path_len),
        Value::I32(new_fd as i32),
        Value::I32(new_path_ptr),
        Value::I32(new_path_len),
    ]
}

fn rename_args(
    old_fd: u32,
    old_path_ptr: i32,
    old_path_len: i32,
    new_fd: u32,
    new_path_ptr: i32,
    new_path_len: i32,
) -> [Value; 6] {
    [
        Value::I32(old_fd as i32),
        Value::I32(old_path_ptr),
        Value::I32(old_path_len),
        Value::I32(new_fd as i32),
        Value::I32(new_path_ptr),
        Value::I32(new_path_len),
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
fn rename_replaces_target_without_breaking_source_alias_or_open_target_handle() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"source.bin").unwrap();
    memory.write(96, b"alias.bin").unwrap();
    memory.write(128, b"target.bin").unwrap();
    memory.write(256, &320u32.to_le_bytes()).unwrap();
    memory.write(260, &3u32.to_le_bytes()).unwrap();

    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap()
        .with_writable_file("/sandbox", "source.bin", b"SRC")
        .unwrap()
        .with_writable_file("/sandbox", "target.bin", b"OLD")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi).unwrap();
    let rights = RIGHTS_FD_READ | RIGHTS_FD_SEEK | RIGHTS_FD_FILESTAT_GET;

    assert_eq!(
        errno(&mut vm, "open", &open_args(3, 64, 10, rights, 400)),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(&mut vm, "open", &open_args(3, 128, 10, rights, 404)),
        ERRNO_SUCCESS
    );
    let source_fd = read_u32(&memory, 400);
    let displaced_fd = read_u32(&memory, 404);

    assert_eq!(
        errno(&mut vm, "link", &link_args(3, 64, 10, 3, 96, 9)),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(
            &mut vm,
            "filestat",
            &[Value::I32(source_fd as i32), Value::I32(512)]
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(
            &mut vm,
            "filestat",
            &[Value::I32(displaced_fd as i32), Value::I32(576)]
        ),
        ERRNO_SUCCESS
    );
    let source_before = filestat(&memory, 512);
    let displaced_before = filestat(&memory, 576);
    let source_inode = read_u64(&source_before, 8);
    let displaced_inode = read_u64(&displaced_before, 8);
    assert_ne!(source_inode, displaced_inode);
    assert_eq!(read_u64(&source_before, 24), 2);
    assert_eq!(read_u64(&displaced_before, 24), 1);

    assert_eq!(
        errno(&mut vm, "rename", &rename_args(3, 64, 10, 3, 128, 10)),
        ERRNO_SUCCESS
    );
    assert!(wasi.file_snapshot("/sandbox", "source.bin").is_none());
    assert_eq!(wasi.file_snapshot("/sandbox", "alias.bin").unwrap(), b"SRC");
    assert_eq!(
        wasi.file_snapshot("/sandbox", "target.bin").unwrap(),
        b"SRC"
    );

    assert_eq!(
        errno(
            &mut vm,
            "filestat",
            &[Value::I32(source_fd as i32), Value::I32(512)]
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(
            &mut vm,
            "filestat",
            &[Value::I32(displaced_fd as i32), Value::I32(576)]
        ),
        ERRNO_SUCCESS
    );
    let source_after = filestat(&memory, 512);
    let displaced_after = filestat(&memory, 576);
    assert_eq!(read_u64(&source_after, 8), source_inode);
    assert_eq!(read_u64(&source_after, 24), 2);
    assert_eq!(read_u64(&displaced_after, 8), displaced_inode);
    assert_eq!(read_u64(&displaced_after, 24), 0);

    assert_eq!(
        errno(&mut vm, "open", &open_args(3, 128, 10, rights, 408)),
        ERRNO_SUCCESS
    );
    let replacement_fd = read_u32(&memory, 408);
    assert_eq!(
        errno(
            &mut vm,
            "filestat",
            &[Value::I32(replacement_fd as i32), Value::I32(512)]
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u64(&filestat(&memory, 512), 8), source_inode);

    assert_eq!(
        errno(
            &mut vm,
            "pread",
            &[
                Value::I32(displaced_fd as i32),
                Value::I32(256),
                Value::I32(1),
                Value::I64(0),
                Value::I32(264),
            ]
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u32(&memory, 264), 3);
    assert_eq!(memory.read(320, 3).unwrap(), b"OLD");
}

#[test]
fn rename_rejections_are_atomic_and_same_path_is_a_noop() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"data.bin").unwrap();
    memory.write(96, b"moved.bin").unwrap();
    memory.write(128, b"missing").unwrap();
    memory.write(144, b"../x").unwrap();

    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap()
        .with_preopen("/readonly")
        .unwrap()
        .with_writable_file("/sandbox", "data.bin", b"abc")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi).unwrap();

    assert_eq!(
        errno(&mut vm, "rename", &rename_args(3, 64, 8, 3, 64, 8)),
        ERRNO_SUCCESS
    );
    assert_eq!(wasi.file_snapshot("/sandbox", "data.bin").unwrap(), b"abc");

    for (args, expected) in [
        (rename_args(99, 64, 8, 3, 96, 9), ERRNO_BADF),
        (rename_args(3, 64, 8, 99, 96, 9), ERRNO_BADF),
        (rename_args(4, 64, 8, 3, 96, 9), ERRNO_NOTCAPABLE),
        (rename_args(3, 64, 8, 4, 96, 9), ERRNO_NOTCAPABLE),
        (rename_args(3, 128, 7, 3, 96, 9), ERRNO_NOENT),
        (rename_args(3, 64, 0, 3, 96, 9), ERRNO_INVAL),
        (rename_args(3, 64, 8, 3, 96, 0), ERRNO_INVAL),
        (rename_args(3, 144, 4, 3, 96, 9), ERRNO_NOTCAPABLE),
        (rename_args(3, 64, 8, 3, 144, 4), ERRNO_NOTCAPABLE),
        (rename_args(3, 65_530, 8, 3, 96, 9), ERRNO_FAULT),
        (rename_args(3, 64, 8, 3, 65_530, 9), ERRNO_FAULT),
    ] {
        assert_eq!(errno(&mut vm, "rename", &args), expected);
        assert_eq!(wasi.file_snapshot("/sandbox", "data.bin").unwrap(), b"abc");
        assert!(wasi.file_snapshot("/sandbox", "moved.bin").is_none());
    }
}
