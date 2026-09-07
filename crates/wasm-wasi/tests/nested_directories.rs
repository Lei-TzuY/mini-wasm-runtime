use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, Value};
use wasm_wasi::{
    WasiPreview1, ERRNO_EXIST, ERRNO_FAULT, ERRNO_NOENT, ERRNO_NOTCAPABLE, ERRNO_SUCCESS,
    FILETYPE_DIRECTORY, FILETYPE_REGULAR_FILE, RIGHTS_FD_FILESTAT_GET, RIGHTS_FD_READ,
    RIGHTS_FD_READDIR, RIGHTS_PATH_CREATE_FILE, RIGHTS_PATH_OPEN, RIGHTS_PATH_UNLINK_FILE,
};

const DIRENT_SIZE: usize = 24;
const ERRNO_NOTDIR: i32 = 54;
const ERRNO_NOTEMPTY: i32 = 55;
const RIGHTS_PATH_CREATE_DIRECTORY: u64 = 1 << 9;
const RIGHTS_PATH_REMOVE_DIRECTORY: u64 = 1 << 25;
const OFLAGS_CREAT: u32 = 1 << 0;
const OFLAGS_DIRECTORY: u32 = 1 << 1;

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

fn wrapper_body(params: u8, import_index: u8) -> Vec<u8> {
    let mut body = vec![0];
    for index in 0..params {
        body.extend([0x20, index]);
    }
    body.extend([0x10, import_index, 0x0b]);
    body
}

fn module() -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();

    let mut types = vec![3];
    function_type(&mut types, &[0x7f, 0x7f, 0x7f]);
    function_type(
        &mut types,
        &[0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7e, 0x7e, 0x7f, 0x7f],
    );
    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7e, 0x7f]);
    section(&mut module, 1, &types);

    let mut imports = vec![6];
    for (import_name, type_index) in [
        ("path_create_directory", 0u8),
        ("path_remove_directory", 0),
        ("path_open", 1),
        ("fd_readdir", 2),
        ("path_unlink_file", 0),
    ] {
        name(&mut imports, "wasi_snapshot_preview1");
        name(&mut imports, import_name);
        imports.extend([0, type_index]);
    }
    name(&mut imports, "env");
    name(&mut imports, "memory");
    imports.extend([2, 0, 1]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[5, 0, 0, 1, 2, 0]);

    let mut exports = vec![5];
    for (export_name, function_index) in [
        ("mkdir", 5u8),
        ("rmdir", 6),
        ("open", 7),
        ("readdir", 8),
        ("unlink", 9),
    ] {
        name(&mut exports, export_name);
        exports.extend([0, function_index]);
    }
    section(&mut module, 7, &exports);

    let bodies = [
        wrapper_body(3, 0),
        wrapper_body(3, 1),
        wrapper_body(9, 2),
        wrapper_body(5, 3),
        wrapper_body(3, 4),
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

fn readdir_args(fd: u32, buf: u32, buf_len: u32, cookie: u64, bufused: u32) -> [Value; 5] {
    [
        Value::I32(fd as i32),
        Value::I32(buf as i32),
        Value::I32(buf_len as i32),
        Value::I64(cookie as i64),
        Value::I32(bufused as i32),
    ]
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

#[derive(Debug, PartialEq, Eq)]
struct Dirent {
    inode: u64,
    filetype: u8,
    name: Vec<u8>,
}

fn parse_complete_dirents(bytes: &[u8]) -> Vec<Dirent> {
    let mut cursor = 0;
    let mut entries = Vec::new();
    while bytes.len().saturating_sub(cursor) >= DIRENT_SIZE {
        let header = &bytes[cursor..cursor + DIRENT_SIZE];
        let name_len = read_u32(header, 16) as usize;
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

fn read_dir(
    memory: &MemoryHandle,
    vm: &mut Instance,
    fd: u32,
    buf: u32,
    used_ptr: u32,
) -> Vec<Dirent> {
    assert_eq!(
        errno(vm, "readdir", &readdir_args(fd, buf, 512, 0, used_ptr)),
        ERRNO_SUCCESS
    );
    let used = u32::from_le_bytes(memory.read(used_ptr, 4).unwrap().try_into().unwrap()) as usize;
    parse_complete_dirents(&memory.read(buf, used).unwrap())
}

#[test]
fn nested_directory_lifecycle_is_visible_through_directory_descriptors() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(96, b"docs").unwrap();
    memory.write(112, b"note.txt").unwrap();
    memory.write(128, b"docs/note.txt").unwrap();

    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);

    assert_eq!(errno(&mut vm, "mkdir", &path_args(3, 96, 4)), ERRNO_SUCCESS);

    let directory_rights = RIGHTS_FD_READDIR
        | RIGHTS_PATH_OPEN
        | RIGHTS_PATH_CREATE_FILE
        | RIGHTS_PATH_CREATE_DIRECTORY
        | RIGHTS_PATH_REMOVE_DIRECTORY
        | RIGHTS_PATH_UNLINK_FILE;
    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(3, 96, 4, OFLAGS_DIRECTORY, directory_rights, 32),
        ),
        ERRNO_SUCCESS
    );
    let docs_fd = u32::from_le_bytes(memory.read(32, 4).unwrap().try_into().unwrap());

    let root_entries = read_dir(&memory, &mut vm, 3, 512, 40);
    assert_eq!(
        root_entries
            .iter()
            .map(|entry| (entry.name.as_slice(), entry.filetype))
            .collect::<Vec<_>>(),
        vec![
            (b".".as_slice(), FILETYPE_DIRECTORY),
            (b"..".as_slice(), FILETYPE_DIRECTORY),
            (b"docs".as_slice(), FILETYPE_DIRECTORY),
        ]
    );
    let root_inode = root_entries
        .iter()
        .find(|entry| entry.name.as_slice() == b".")
        .expect("root dot entry")
        .inode;
    let docs_inode = root_entries
        .iter()
        .find(|entry| entry.name.as_slice() == b"docs")
        .expect("docs entry")
        .inode;

    let file_rights = RIGHTS_FD_READ | RIGHTS_FD_FILESTAT_GET;
    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(docs_fd, 112, 8, OFLAGS_CREAT, file_rights, 36),
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        wasi.file_snapshot("/sandbox", "docs/note.txt"),
        Some(Vec::new())
    );

    let docs_entries = read_dir(&memory, &mut vm, docs_fd, 1536, 44);
    assert_eq!(
        docs_entries
            .iter()
            .map(|entry| (entry.name.as_slice(), entry.filetype))
            .collect::<Vec<_>>(),
        vec![
            (b".".as_slice(), FILETYPE_DIRECTORY),
            (b"..".as_slice(), FILETYPE_DIRECTORY),
            (b"note.txt".as_slice(), FILETYPE_REGULAR_FILE),
        ]
    );
    assert_eq!(
        docs_entries
            .iter()
            .find(|entry| entry.name.as_slice() == b".")
            .expect("nested dot entry")
            .inode,
        docs_inode,
        "nested dot must retain the opened directory inode"
    );
    assert_eq!(
        docs_entries
            .iter()
            .find(|entry| entry.name.as_slice() == b"..")
            .expect("nested dot-dot entry")
            .inode,
        root_inode,
        "nested dot-dot must identify the parent directory"
    );

    assert_eq!(
        errno(&mut vm, "rmdir", &path_args(3, 96, 4)),
        ERRNO_NOTEMPTY
    );
    assert_eq!(
        errno(&mut vm, "unlink", &path_args(docs_fd, 112, 8)),
        ERRNO_SUCCESS
    );
    assert_eq!(
        read_dir(&memory, &mut vm, docs_fd, 2048, 52)
            .into_iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>(),
        vec![b".".to_vec(), b"..".to_vec()]
    );
    assert_eq!(errno(&mut vm, "rmdir", &path_args(3, 96, 4)), ERRNO_SUCCESS);

    let root_entries = read_dir(&memory, &mut vm, 3, 2560, 48);
    assert_eq!(
        root_entries
            .iter()
            .map(|entry| entry.name.as_slice())
            .collect::<Vec<_>>(),
        vec![b".".as_slice(), b"..".as_slice()]
    );
}

#[test]
fn directory_mutation_failures_are_atomic_and_capability_scoped() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(96, b"docs").unwrap();
    memory.write(112, b"missing/child").unwrap();
    memory.write(144, b"../escape").unwrap();
    memory.write(176, b"plain.txt").unwrap();

    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap()
        .with_writable_file("/sandbox", "plain.txt", b"x")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);

    assert_eq!(errno(&mut vm, "mkdir", &path_args(3, 96, 4)), ERRNO_SUCCESS);
    assert_eq!(errno(&mut vm, "mkdir", &path_args(3, 96, 4)), ERRNO_EXIST);
    assert_eq!(errno(&mut vm, "mkdir", &path_args(3, 112, 13)), ERRNO_NOENT);
    assert_eq!(
        errno(&mut vm, "mkdir", &path_args(3, 144, 9)),
        ERRNO_NOTCAPABLE
    );
    assert_eq!(
        errno(&mut vm, "mkdir", &path_args(3, 65_534, 4)),
        ERRNO_FAULT
    );
    assert_eq!(errno(&mut vm, "rmdir", &path_args(3, 112, 7)), ERRNO_NOENT);

    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(3, 176, 9, OFLAGS_DIRECTORY, RIGHTS_FD_READDIR, 32),
        ),
        ERRNO_NOTDIR
    );

    let entries = read_dir(&memory, &mut vm, 3, 512, 40);
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.name.as_slice())
            .collect::<Vec<_>>(),
        vec![
            b".".as_slice(),
            b"..".as_slice(),
            b"docs".as_slice(),
            b"plain.txt".as_slice()
        ]
    );
}

#[test]
fn readonly_preopens_cannot_mutate_directory_namespace() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(96, b"docs").unwrap();
    let wasi = WasiPreview1::new().with_preopen("/sandbox").unwrap();
    let mut vm = instantiate(&memory, &wasi);

    assert_eq!(
        errno(&mut vm, "mkdir", &path_args(3, 96, 4)),
        ERRNO_NOTCAPABLE
    );
    assert_eq!(
        errno(&mut vm, "rmdir", &path_args(3, 96, 4)),
        ERRNO_NOTCAPABLE
    );
    assert_eq!(
        read_dir(&memory, &mut vm, 3, 512, 40)
            .into_iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>(),
        vec![b".".to_vec(), b"..".to_vec()]
    );
}
