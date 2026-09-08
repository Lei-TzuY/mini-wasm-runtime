use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, Value};
use wasm_wasi::{
    WasiPreview1, ERRNO_BADF, ERRNO_FAULT, ERRNO_INVAL, ERRNO_NOENT, ERRNO_NOTCAPABLE,
    ERRNO_NOTSUP, ERRNO_SUCCESS, FILETYPE_DIRECTORY, FILETYPE_REGULAR_FILE, FILETYPE_SYMBOLIC_LINK,
    OFLAGS_CREAT, OFLAGS_DIRECTORY, RIGHTS_PATH_CREATE_FILE, RIGHTS_PATH_FILESTAT_GET,
    RIGHTS_PATH_OPEN,
};

const LOOKUPFLAGS_SYMLINK_FOLLOW: u32 = 1 << 0;
const FILESTAT_SIZE: usize = 64;

const DATA_PATH: u32 = 64;
const ALIAS_PATH: u32 = 80;
const DIR_PATH: u32 = 96;
const LINK_PATH: u32 = 112;
const LINK_TARGET: u32 = 128;
const CHILD_PATH: u32 = 144;
const MISSING_PATH: u32 = 160;
const TRAVERSAL_PATH: u32 = 176;
const OPENED_FD: u32 = 256;
const SECOND_FD: u32 = 264;
const FILESTAT_A: u32 = 512;
const FILESTAT_B: u32 = 640;

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
    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7f, 0x7f]);
    function_type(&mut types, &[0x7f, 0x7f, 0x7f]);
    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f]);
    function_type(
        &mut types,
        &[0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7e, 0x7e, 0x7f, 0x7f],
    );
    section(&mut module, 1, &types);

    let mut imports = vec![6];
    add_function_import(&mut imports, "path_filestat_get", 0);
    add_function_import(&mut imports, "path_create_directory", 1);
    add_function_import(&mut imports, "path_symlink", 0);
    add_function_import(&mut imports, "path_link", 2);
    add_function_import(&mut imports, "path_open", 3);
    name(&mut imports, "env");
    name(&mut imports, "memory");
    imports.extend([2, 0, 1]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[5, 0, 1, 0, 2, 3]);

    let mut exports = vec![5];
    for (export_name, function_index) in [
        ("filestat", 5),
        ("mkdir", 6),
        ("symlink", 7),
        ("link", 8),
        ("open", 9),
    ] {
        name(&mut exports, export_name);
        exports.push(0);
        u32leb(&mut exports, function_index);
    }
    section(&mut module, 7, &exports);

    let bodies = [
        forwarder(5, 0),
        forwarder(3, 1),
        forwarder(5, 2),
        forwarder(7, 3),
        forwarder(9, 4),
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

fn stat_args(fd: u32, flags: u32, path_ptr: u32, path_len: u32, out: u32) -> Vec<Value> {
    vec![
        Value::I32(fd as i32),
        Value::I32(flags as i32),
        Value::I32(path_ptr as i32),
        Value::I32(path_len as i32),
        Value::I32(out as i32),
    ]
}

fn open_args(
    fd: u32,
    path_ptr: u32,
    path_len: u32,
    open_flags: u32,
    rights: u64,
    opened_fd_ptr: u32,
) -> Vec<Value> {
    vec![
        Value::I32(fd as i32),
        Value::I32(0),
        Value::I32(path_ptr as i32),
        Value::I32(path_len as i32),
        Value::I32(open_flags as i32),
        Value::I64(rights as i64),
        Value::I64(0),
        Value::I32(0),
        Value::I32(opened_fd_ptr as i32),
    ]
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn filestat(memory: &MemoryHandle, ptr: u32) -> Vec<u8> {
    memory.read(ptr, FILESTAT_SIZE).unwrap()
}

fn write_common_paths(memory: &MemoryHandle) {
    memory.write(DATA_PATH, b"data.bin").unwrap();
    memory.write(ALIAS_PATH, b"alias.bin").unwrap();
    memory.write(DIR_PATH, b"dir").unwrap();
    memory.write(LINK_PATH, b"link").unwrap();
    memory.write(LINK_TARGET, b"data.bin").unwrap();
    memory.write(CHILD_PATH, b"child.bin").unwrap();
    memory.write(MISSING_PATH, b"missing").unwrap();
    memory.write(TRAVERSAL_PATH, b"../x").unwrap();
}

#[test]
fn path_filestat_get_reports_regular_and_hard_link_metadata() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    write_common_paths(&memory);
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap()
        .with_writable_file("/sandbox", "data.bin", b"abc")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);

    assert_eq!(
        errno(
            &mut vm,
            "filestat",
            &stat_args(3, 0, DATA_PATH, 8, FILESTAT_A)
        ),
        ERRNO_SUCCESS
    );
    let initial = filestat(&memory, FILESTAT_A);
    let dev = read_u64(&initial, 0);
    let ino = read_u64(&initial, 8);
    assert_ne!(dev, 0);
    assert_ne!(ino, 0);
    assert_eq!(initial[16], FILETYPE_REGULAR_FILE);
    assert_eq!(read_u64(&initial, 24), 1);
    assert_eq!(read_u64(&initial, 32), 3);
    assert_eq!(&initial[40..64], &[0; 24]);

    assert_eq!(
        errno(
            &mut vm,
            "link",
            &[
                Value::I32(3),
                Value::I32(0),
                Value::I32(DATA_PATH as i32),
                Value::I32(8),
                Value::I32(3),
                Value::I32(ALIAS_PATH as i32),
                Value::I32(9),
            ]
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(
            &mut vm,
            "filestat",
            &stat_args(3, 0, ALIAS_PATH, 9, FILESTAT_B)
        ),
        ERRNO_SUCCESS
    );
    let linked = filestat(&memory, FILESTAT_B);
    assert_eq!(read_u64(&linked, 0), dev);
    assert_eq!(read_u64(&linked, 8), ino);
    assert_eq!(linked[16], FILETYPE_REGULAR_FILE);
    assert_eq!(read_u64(&linked, 24), 2);
    assert_eq!(read_u64(&linked, 32), 3);

    assert_eq!(
        errno(
            &mut vm,
            "filestat",
            &stat_args(3, 0, DATA_PATH, 8, FILESTAT_A)
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u64(&filestat(&memory, FILESTAT_A), 24), 2);
}

#[test]
fn path_filestat_get_reports_directories_and_nonfollowing_symlinks() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    write_common_paths(&memory);
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap()
        .with_writable_file("/sandbox", "data.bin", b"abc")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);

    assert_eq!(
        errno(
            &mut vm,
            "mkdir",
            &[Value::I32(3), Value::I32(DIR_PATH as i32), Value::I32(3)]
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(
            &mut vm,
            "symlink",
            &[
                Value::I32(LINK_TARGET as i32),
                Value::I32(8),
                Value::I32(3),
                Value::I32(LINK_PATH as i32),
                Value::I32(4),
            ]
        ),
        ERRNO_SUCCESS
    );

    assert_eq!(
        errno(
            &mut vm,
            "filestat",
            &stat_args(3, 0, DIR_PATH, 3, FILESTAT_A)
        ),
        ERRNO_SUCCESS
    );
    let directory = filestat(&memory, FILESTAT_A);
    assert_eq!(directory[16], FILETYPE_DIRECTORY);
    assert_ne!(read_u64(&directory, 8), 0);
    assert_eq!(read_u64(&directory, 24), 1);
    assert_eq!(read_u64(&directory, 32), 0);

    assert_eq!(
        errno(
            &mut vm,
            "filestat",
            &stat_args(3, 0, LINK_PATH, 4, FILESTAT_A)
        ),
        ERRNO_SUCCESS
    );
    let link = filestat(&memory, FILESTAT_A);
    assert_eq!(link[16], FILETYPE_SYMBOLIC_LINK);
    assert_ne!(read_u64(&link, 8), 0);
    assert_eq!(read_u64(&link, 24), 1);
    assert_eq!(read_u64(&link, 32), 8);

    memory.write(FILESTAT_B, &[0xa5; FILESTAT_SIZE]).unwrap();
    assert_eq!(
        errno(
            &mut vm,
            "filestat",
            &stat_args(3, LOOKUPFLAGS_SYMLINK_FOLLOW, LINK_PATH, 4, FILESTAT_B)
        ),
        ERRNO_NOTSUP
    );
    assert_eq!(filestat(&memory, FILESTAT_B), vec![0xa5; FILESTAT_SIZE]);
}

#[test]
fn path_filestat_get_is_directory_relative_and_fail_closed() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    write_common_paths(&memory);
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);

    assert_eq!(
        errno(
            &mut vm,
            "mkdir",
            &[Value::I32(3), Value::I32(DIR_PATH as i32), Value::I32(3)]
        ),
        ERRNO_SUCCESS
    );
    let dir_rights = RIGHTS_PATH_FILESTAT_GET | RIGHTS_PATH_OPEN | RIGHTS_PATH_CREATE_FILE;
    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(3, DIR_PATH, 3, OFLAGS_DIRECTORY, dir_rights, OPENED_FD)
        ),
        ERRNO_SUCCESS
    );
    let dir_fd = u32::from_le_bytes(memory.read(OPENED_FD, 4).unwrap().try_into().unwrap());

    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(dir_fd, CHILD_PATH, 9, OFLAGS_CREAT, 0, SECOND_FD)
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(
            &mut vm,
            "filestat",
            &stat_args(dir_fd, 0, CHILD_PATH, 9, FILESTAT_A)
        ),
        ERRNO_SUCCESS
    );
    let child = filestat(&memory, FILESTAT_A);
    assert_eq!(child[16], FILETYPE_REGULAR_FILE);
    assert_eq!(read_u64(&child, 24), 1);
    assert_eq!(read_u64(&child, 32), 0);

    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(3, DIR_PATH, 3, OFLAGS_DIRECTORY, 0, SECOND_FD)
        ),
        ERRNO_SUCCESS
    );
    let restricted_fd = u32::from_le_bytes(memory.read(SECOND_FD, 4).unwrap().try_into().unwrap());
    memory.write(FILESTAT_B, &[0x5a; FILESTAT_SIZE]).unwrap();
    assert_eq!(
        errno(
            &mut vm,
            "filestat",
            &stat_args(restricted_fd, 0, CHILD_PATH, 9, FILESTAT_B)
        ),
        ERRNO_NOTCAPABLE
    );
    assert_eq!(filestat(&memory, FILESTAT_B), vec![0x5a; FILESTAT_SIZE]);

    for (fd, flags, path_ptr, path_len, expected) in [
        (999, 0, CHILD_PATH, 9, ERRNO_BADF),
        (dir_fd, 0, MISSING_PATH, 7, ERRNO_NOENT),
        (dir_fd, 2, CHILD_PATH, 9, ERRNO_INVAL),
        (dir_fd, 0, TRAVERSAL_PATH, 4, ERRNO_NOTCAPABLE),
    ] {
        memory.write(FILESTAT_B, &[0x3c; FILESTAT_SIZE]).unwrap();
        assert_eq!(
            errno(
                &mut vm,
                "filestat",
                &stat_args(fd, flags, path_ptr, path_len, FILESTAT_B)
            ),
            expected
        );
        assert_eq!(filestat(&memory, FILESTAT_B), vec![0x3c; FILESTAT_SIZE]);
    }

    memory.write(65_520, &[0x7e; 16]).unwrap();
    assert_eq!(
        errno(
            &mut vm,
            "filestat",
            &stat_args(dir_fd, 0, CHILD_PATH, 9, 65_520)
        ),
        ERRNO_FAULT
    );
    assert_eq!(memory.read(65_520, 16).unwrap(), vec![0x7e; 16]);
}
