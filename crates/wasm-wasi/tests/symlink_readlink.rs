use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, Value};
use wasm_wasi::{
    WasiPreview1, ERRNO_FAULT, ERRNO_NOENT, ERRNO_NOTCAPABLE, ERRNO_SUCCESS, FILETYPE_SYMBOLIC_LINK,
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
    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7f, 0x7f]);
    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f]);
    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7e, 0x7f]);
    function_type(&mut types, &[0x7f, 0x7f, 0x7f]);
    section(&mut module, 1, &types);

    let mut imports = vec![5];
    add_function_import(&mut imports, "path_symlink", 0);
    add_function_import(&mut imports, "path_readlink", 1);
    add_function_import(&mut imports, "fd_readdir", 2);
    add_function_import(&mut imports, "path_unlink_file", 3);
    name(&mut imports, "env");
    name(&mut imports, "memory");
    imports.extend([2, 0, 1]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[4, 0, 1, 2, 3]);

    let mut exports = vec![4];
    for (export_name, function_index) in [
        ("symlink", 4u32),
        ("readlink", 5),
        ("readdir", 6),
        ("unlink", 7),
    ] {
        name(&mut exports, export_name);
        exports.push(0);
        u32leb(&mut exports, function_index);
    }
    section(&mut module, 7, &exports);

    let bodies = [
        wrapper_body(5, 0),
        wrapper_body(6, 1),
        wrapper_body(5, 2),
        wrapper_body(3, 3),
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

fn symlink_args(
    target_ptr: u32,
    target_len: u32,
    fd: u32,
    path_ptr: u32,
    path_len: u32,
) -> [Value; 5] {
    [
        Value::I32(target_ptr as i32),
        Value::I32(target_len as i32),
        Value::I32(fd as i32),
        Value::I32(path_ptr as i32),
        Value::I32(path_len as i32),
    ]
}

fn readlink_args(
    fd: u32,
    path_ptr: u32,
    path_len: u32,
    buf: u32,
    buf_len: u32,
    bufused: u32,
) -> [Value; 6] {
    [
        Value::I32(fd as i32),
        Value::I32(path_ptr as i32),
        Value::I32(path_len as i32),
        Value::I32(buf as i32),
        Value::I32(buf_len as i32),
        Value::I32(bufused as i32),
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

fn path_args(fd: u32, ptr: u32, len: u32) -> [Value; 3] {
    [
        Value::I32(fd as i32),
        Value::I32(ptr as i32),
        Value::I32(len as i32),
    ]
}

fn read_u32(memory: &MemoryHandle, ptr: u32) -> u32 {
    u32::from_le_bytes(memory.read(ptr, 4).unwrap().try_into().unwrap())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Dirent {
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
        let start = cursor + DIRENT_SIZE;
        let end = start + name_len;
        entries.push(Dirent {
            filetype: header[20],
            name: bytes[start..end].to_vec(),
        });
        cursor = end;
    }
    entries
}

#[test]
fn symlink_readlink_readdir_unlink_lifecycle_is_executable() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let target = b"../outside/data.bin";
    let link = b"shortcut";
    memory.write(64, target).unwrap();
    memory.write(96, link).unwrap();
    memory.write(512, &[0xa5; 128]).unwrap();

    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);

    assert_eq!(
        errno(
            &mut vm,
            "symlink",
            &symlink_args(64, target.len() as u32, 3, 96, link.len() as u32),
        ),
        ERRNO_SUCCESS
    );

    assert_eq!(
        errno(
            &mut vm,
            "readlink",
            &readlink_args(3, 96, link.len() as u32, 512, 64, 480),
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u32(&memory, 480), target.len() as u32);
    assert_eq!(memory.read(512, target.len()).unwrap(), target);
    assert_eq!(
        memory.read(512 + target.len() as u32, 1).unwrap(),
        vec![0xa5]
    );

    assert_eq!(
        errno(&mut vm, "readdir", &readdir_args(3, 1024, 512, 0, 484)),
        ERRNO_SUCCESS
    );
    let used = read_u32(&memory, 484) as usize;
    let entries = parse_dirents(&memory.read(1024, used).unwrap());
    let entry = entries
        .iter()
        .find(|entry| entry.name.as_slice() == link)
        .expect("symlink dirent");
    assert_eq!(entry.filetype, FILETYPE_SYMBOLIC_LINK);

    assert_eq!(
        errno(&mut vm, "unlink", &path_args(3, 96, link.len() as u32)),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(
            &mut vm,
            "readlink",
            &readlink_args(3, 96, link.len() as u32, 512, 64, 480),
        ),
        ERRNO_NOENT
    );
}

#[test]
fn readlink_truncates_without_nul_termination_and_preserves_adjacent_memory() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let target = b"nested/very-long-target";
    let link = b"tiny";
    memory.write(64, target).unwrap();
    memory.write(96, link).unwrap();
    memory.write(512, &[0xcc; 16]).unwrap();

    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);

    assert_eq!(
        errno(
            &mut vm,
            "symlink",
            &symlink_args(64, target.len() as u32, 3, 96, link.len() as u32),
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(
            &mut vm,
            "readlink",
            &readlink_args(3, 96, link.len() as u32, 512, 5, 480),
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u32(&memory, 480), 5);
    assert_eq!(memory.read(512, 5).unwrap(), target[..5]);
    assert_eq!(memory.read(517, 1).unwrap(), vec![0xcc]);
}

#[test]
fn symlink_and_readlink_fail_closed_on_capability_and_memory_faults() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let target = b"target";
    let link = b"link";
    memory.write(64, target).unwrap();
    memory.write(96, link).unwrap();

    let readonly = WasiPreview1::new().with_preopen("/sandbox").unwrap();
    let mut readonly_vm = instantiate(&memory, &readonly);
    assert_eq!(
        errno(
            &mut readonly_vm,
            "symlink",
            &symlink_args(64, target.len() as u32, 3, 96, link.len() as u32),
        ),
        ERRNO_NOTCAPABLE
    );

    let writable = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap();
    let mut writable_vm = instantiate(&memory, &writable);
    assert_eq!(
        errno(
            &mut writable_vm,
            "symlink",
            &symlink_args(64, target.len() as u32, 3, 96, link.len() as u32),
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(
            &mut writable_vm,
            "readlink",
            &readlink_args(3, 96, link.len() as u32, 65_530, 16, 480),
        ),
        ERRNO_FAULT
    );
    assert_eq!(
        errno(
            &mut writable_vm,
            "readlink",
            &readlink_args(3, 96, link.len() as u32, 512, 16, 65_534),
        ),
        ERRNO_FAULT
    );
}
