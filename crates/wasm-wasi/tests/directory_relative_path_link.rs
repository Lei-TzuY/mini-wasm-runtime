use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, Value};
use wasm_wasi::{
    WasiPreview1, ERRNO_BADF, ERRNO_EXIST, ERRNO_FAULT, ERRNO_NOENT, ERRNO_NOTCAPABLE,
    ERRNO_SUCCESS, FILETYPE_DIRECTORY, FILETYPE_REGULAR_FILE, OFLAGS_CREAT, OFLAGS_DIRECTORY,
    RIGHTS_FD_FILESTAT_GET, RIGHTS_FD_READ, RIGHTS_FD_READDIR, RIGHTS_PATH_CREATE_DIRECTORY,
    RIGHTS_PATH_CREATE_FILE, RIGHTS_PATH_LINK_SOURCE, RIGHTS_PATH_LINK_TARGET, RIGHTS_PATH_OPEN,
    RIGHTS_PATH_UNLINK_FILE,
};

const DIRENT_SIZE: usize = 24;

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

fn function_type(out: &mut Vec<u8>, params: &[u8]) {
    out.push(0x60);
    u32leb(out, params.len() as u32);
    out.extend_from_slice(params);
    out.extend([1, 0x7f]);
}

fn add_function_import(imports: &mut Vec<u8>, function: &str, type_index: u32) {
    name(imports, "wasi_snapshot_preview1");
    name(imports, function);
    imports.push(0);
    u32leb(imports, type_index);
}

fn wrapper_body(param_count: u32, import_index: u32) -> Vec<u8> {
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
    function_type(&mut types, &[0x7f, 0x7f, 0x7f]);
    function_type(
        &mut types,
        &[0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7e, 0x7e, 0x7f, 0x7f],
    );
    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f]);
    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7e, 0x7f]);
    section(&mut module, 1, &types);

    let mut imports = vec![6];
    add_function_import(&mut imports, "path_create_directory", 0);
    add_function_import(&mut imports, "path_open", 1);
    add_function_import(&mut imports, "path_link", 2);
    add_function_import(&mut imports, "path_unlink_file", 0);
    add_function_import(&mut imports, "fd_readdir", 3);
    name(&mut imports, "env");
    name(&mut imports, "memory");
    imports.extend([2, 0, 1]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[5, 0, 1, 2, 0, 3]);

    let mut exports = vec![5];
    for (export_name, function_index) in [
        ("mkdir", 5u32),
        ("open", 6),
        ("link", 7),
        ("unlink", 8),
        ("readdir", 9),
    ] {
        name(&mut exports, export_name);
        exports.push(0);
        u32leb(&mut exports, function_index);
    }
    section(&mut module, 7, &exports);

    let bodies = [
        wrapper_body(3, 0),
        wrapper_body(9, 1),
        wrapper_body(7, 2),
        wrapper_body(3, 3),
        wrapper_body(5, 4),
    ];
    let mut code = vec![bodies.len() as u8];
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

fn path_args(fd: u32, ptr: u32, len: u32) -> [Value; 3] {
    [
        Value::I32(fd as i32),
        Value::I32(ptr as i32),
        Value::I32(len as i32),
    ]
}

fn open_args(
    fd: u32,
    path_ptr: u32,
    path_len: u32,
    oflags: u32,
    rights: u64,
    opened_fd: u32,
) -> [Value; 9] {
    [
        Value::I32(fd as i32),
        Value::I32(0),
        Value::I32(path_ptr as i32),
        Value::I32(path_len as i32),
        Value::I32(oflags as i32),
        Value::I64(rights as i64),
        Value::I64(0),
        Value::I32(0),
        Value::I32(opened_fd as i32),
    ]
}

fn link_args(
    old_fd: u32,
    old_flags: u32,
    old_path_ptr: u32,
    old_path_len: u32,
    new_fd: u32,
    new_path_ptr: u32,
    new_path_len: u32,
) -> [Value; 7] {
    [
        Value::I32(old_fd as i32),
        Value::I32(old_flags as i32),
        Value::I32(old_path_ptr as i32),
        Value::I32(old_path_len as i32),
        Value::I32(new_fd as i32),
        Value::I32(new_path_ptr as i32),
        Value::I32(new_path_len as i32),
    ]
}

fn readdir_args(fd: u32, buf: u32, buf_len: u32, cookie: u64, bufused: u32) -> [Value; 5] {
    [
        Value::I32(fd as i32),
        Value::I32(buf as i32),
        Value::I32(buf_len as i32),
        Value::I64(cookie as i64),
        Value::I32(bufused as i32),
    ]
}

fn read_u32(memory: &MemoryHandle, ptr: u32) -> u32 {
    u32::from_le_bytes(memory.read(ptr, 4).unwrap().try_into().unwrap())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Dirent {
    inode: u64,
    filetype: u8,
    name: Vec<u8>,
}

fn parse_dirents(bytes: &[u8]) -> Vec<Dirent> {
    let mut cursor = 0;
    let mut entries = Vec::new();
    while bytes.len().saturating_sub(cursor) >= DIRENT_SIZE {
        let header = &bytes[cursor..cursor + DIRENT_SIZE];
        let name_len = u32::from_le_bytes(header[16..20].try_into().unwrap()) as usize;
        if bytes.len().saturating_sub(cursor + DIRENT_SIZE) < name_len {
            break;
        }
        let name_start = cursor + DIRENT_SIZE;
        let name_end = name_start + name_len;
        entries.push(Dirent {
            inode: u64::from_le_bytes(header[8..16].try_into().unwrap()),
            filetype: header[20],
            name: bytes[name_start..name_end].to_vec(),
        });
        cursor = name_end;
    }
    entries
}

fn read_dir(memory: &MemoryHandle, vm: &mut Instance, fd: u32, buf: u32, used: u32) -> Vec<Dirent> {
    assert_eq!(
        errno(vm, "readdir", &readdir_args(fd, buf, 512, 0, used)),
        ERRNO_SUCCESS
    );
    let used = read_u32(memory, used) as usize;
    parse_dirents(&memory.read(buf, used).unwrap())
}

fn directory_rights() -> u64 {
    RIGHTS_FD_READDIR
        | RIGHTS_PATH_OPEN
        | RIGHTS_PATH_CREATE_DIRECTORY
        | RIGHTS_PATH_CREATE_FILE
        | RIGHTS_PATH_LINK_SOURCE
        | RIGHTS_PATH_LINK_TARGET
        | RIGHTS_PATH_UNLINK_FILE
}

#[test]
fn opened_directories_can_link_files_across_nested_namespaces() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"left").unwrap();
    memory.write(80, b"right").unwrap();
    memory.write(96, b"note.txt").unwrap();
    memory.write(112, b"alias.txt").unwrap();

    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);

    assert_eq!(errno(&mut vm, "mkdir", &path_args(3, 64, 4)), ERRNO_SUCCESS);
    assert_eq!(errno(&mut vm, "mkdir", &path_args(3, 80, 5)), ERRNO_SUCCESS);

    let rights = directory_rights();
    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(3, 64, 4, OFLAGS_DIRECTORY, rights, 300),
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(3, 80, 5, OFLAGS_DIRECTORY, rights, 304),
        ),
        ERRNO_SUCCESS
    );
    let left_fd = read_u32(&memory, 300);
    let right_fd = read_u32(&memory, 304);

    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(
                left_fd,
                96,
                8,
                OFLAGS_CREAT,
                RIGHTS_FD_READ | RIGHTS_FD_FILESTAT_GET,
                308,
            ),
        ),
        ERRNO_SUCCESS
    );

    assert_eq!(
        errno(
            &mut vm,
            "link",
            &link_args(left_fd, 0, 96, 8, right_fd, 112, 9),
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        wasi.file_snapshot("/sandbox", "left/note.txt"),
        Some(Vec::new())
    );
    assert_eq!(
        wasi.file_snapshot("/sandbox", "right/alias.txt"),
        Some(Vec::new())
    );

    let left_entries = read_dir(&memory, &mut vm, left_fd, 1024, 340);
    let right_entries = read_dir(&memory, &mut vm, right_fd, 2048, 344);
    let source = left_entries
        .iter()
        .find(|entry| entry.name.as_slice() == b"note.txt")
        .expect("source dirent");
    let alias = right_entries
        .iter()
        .find(|entry| entry.name.as_slice() == b"alias.txt")
        .expect("alias dirent");
    assert_eq!(source.filetype, FILETYPE_REGULAR_FILE);
    assert_eq!(alias.filetype, FILETYPE_REGULAR_FILE);
    assert_eq!(
        source.inode, alias.inode,
        "hard links must share inode identity"
    );

    assert_eq!(
        errno(&mut vm, "unlink", &path_args(left_fd, 96, 8)),
        ERRNO_SUCCESS
    );
    assert!(wasi.file_snapshot("/sandbox", "left/note.txt").is_none());
    assert_eq!(
        wasi.file_snapshot("/sandbox", "right/alias.txt"),
        Some(Vec::new())
    );

    assert_eq!(
        errno(&mut vm, "unlink", &path_args(right_fd, 112, 9)),
        ERRNO_SUCCESS
    );
    assert!(wasi.file_snapshot("/sandbox", "right/alias.txt").is_none());
}

#[test]
fn directory_relative_link_failures_are_atomic_and_capability_scoped() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"left").unwrap();
    memory.write(80, b"right").unwrap();
    memory.write(96, b"note.txt").unwrap();
    memory.write(112, b"alias.txt").unwrap();
    memory.write(128, b"taken").unwrap();
    memory.write(144, b"missing/alias").unwrap();
    memory.write(176, b"../escape").unwrap();

    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);

    assert_eq!(errno(&mut vm, "mkdir", &path_args(3, 64, 4)), ERRNO_SUCCESS);
    assert_eq!(errno(&mut vm, "mkdir", &path_args(3, 80, 5)), ERRNO_SUCCESS);

    let full_rights = directory_rights();
    let source_limited = RIGHTS_FD_READDIR | RIGHTS_PATH_OPEN | RIGHTS_PATH_CREATE_FILE;
    let target_limited = RIGHTS_FD_READDIR | RIGHTS_PATH_OPEN;
    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(3, 64, 4, OFLAGS_DIRECTORY, source_limited, 300),
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(3, 80, 5, OFLAGS_DIRECTORY, target_limited, 304),
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(3, 64, 4, OFLAGS_DIRECTORY, full_rights, 308),
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(3, 80, 5, OFLAGS_DIRECTORY, full_rights, 312),
        ),
        ERRNO_SUCCESS
    );
    let source_limited_fd = read_u32(&memory, 300);
    let target_limited_fd = read_u32(&memory, 304);
    let source_fd = read_u32(&memory, 308);
    let target_fd = read_u32(&memory, 312);

    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(source_limited_fd, 96, 8, OFLAGS_CREAT, RIGHTS_FD_READ, 316,),
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(&mut vm, "mkdir", &path_args(target_fd, 128, 5)),
        ERRNO_SUCCESS
    );

    let rejected = [
        (link_args(99, 0, 96, 8, target_fd, 112, 9), ERRNO_BADF),
        (link_args(source_fd, 0, 96, 8, 99, 112, 9), ERRNO_BADF),
        (
            link_args(source_limited_fd, 0, 96, 8, target_fd, 112, 9),
            ERRNO_NOTCAPABLE,
        ),
        (
            link_args(source_fd, 0, 96, 8, target_limited_fd, 112, 9),
            ERRNO_NOTCAPABLE,
        ),
        (
            link_args(source_fd, 0, 96, 8, target_fd, 128, 5),
            ERRNO_EXIST,
        ),
        (
            link_args(source_fd, 0, 96, 8, target_fd, 144, 13),
            ERRNO_NOENT,
        ),
        (
            link_args(source_fd, 0, 96, 8, target_fd, 176, 9),
            ERRNO_NOTCAPABLE,
        ),
        (
            link_args(source_fd, 0, 96, 8, target_fd, 65_530, 9),
            ERRNO_FAULT,
        ),
    ];
    for (args, expected) in rejected {
        assert_eq!(errno(&mut vm, "link", &args), expected);
        assert!(wasi.file_snapshot("/sandbox", "right/alias.txt").is_none());
        assert_eq!(
            wasi.file_snapshot("/sandbox", "left/note.txt"),
            Some(Vec::new())
        );
    }

    let target_entries = read_dir(&memory, &mut vm, target_fd, 1024, 340);
    assert_eq!(
        target_entries
            .iter()
            .map(|entry| (entry.name.as_slice(), entry.filetype))
            .collect::<Vec<_>>(),
        vec![
            (b".".as_slice(), FILETYPE_DIRECTORY),
            (b"..".as_slice(), FILETYPE_DIRECTORY),
            (b"taken".as_slice(), FILETYPE_DIRECTORY),
        ]
    );
}
