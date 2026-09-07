use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, Value};
use wasm_wasi::{
    WasiPreview1, ERRNO_NOTCAPABLE, ERRNO_SUCCESS, OFLAGS_DIRECTORY, RIGHTS_FD_READDIR,
    RIGHTS_PATH_READLINK, RIGHTS_PATH_SYMLINK,
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

fn function_type(out: &mut Vec<u8>, params: &[u8]) {
    out.push(0x60);
    u32leb(out, params.len() as u32);
    out.extend_from_slice(params);
    out.extend([1, 0x7f]);
}

fn add_import(imports: &mut Vec<u8>, function: &str, type_index: u32) {
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
    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7f, 0x7f]);
    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f]);
    section(&mut module, 1, &types);

    let mut imports = vec![5];
    add_import(&mut imports, "path_create_directory", 0);
    add_import(&mut imports, "path_open", 1);
    add_import(&mut imports, "path_symlink", 2);
    add_import(&mut imports, "path_readlink", 3);
    name(&mut imports, "env");
    name(&mut imports, "memory");
    imports.extend([2, 0, 1]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[4, 0, 1, 2, 3]);

    let mut exports = vec![4];
    for (export_name, function_index) in [
        ("mkdir", 4u32),
        ("open", 5),
        ("symlink", 6),
        ("readlink", 7),
    ] {
        name(&mut exports, export_name);
        exports.push(0);
        u32leb(&mut exports, function_index);
    }
    section(&mut module, 7, &exports);

    let bodies = [
        wrapper_body(3, 0),
        wrapper_body(9, 1),
        wrapper_body(5, 2),
        wrapper_body(6, 3),
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

fn open_args(fd: u32, path_ptr: u32, path_len: u32, rights: u64, opened_fd: u32) -> [Value; 9] {
    [
        Value::I32(fd as i32),
        Value::I32(0),
        Value::I32(path_ptr as i32),
        Value::I32(path_len as i32),
        Value::I32(OFLAGS_DIRECTORY as i32),
        Value::I64(rights as i64),
        Value::I64(0),
        Value::I32(0),
        Value::I32(opened_fd as i32),
    ]
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
    used: u32,
) -> [Value; 6] {
    [
        Value::I32(fd as i32),
        Value::I32(path_ptr as i32),
        Value::I32(path_len as i32),
        Value::I32(buf as i32),
        Value::I32(buf_len as i32),
        Value::I32(used as i32),
    ]
}

fn read_u32(memory: &MemoryHandle, ptr: u32) -> u32 {
    u32::from_le_bytes(memory.read(ptr, 4).unwrap().try_into().unwrap())
}

#[test]
fn opened_directory_rights_scope_symlink_and_readlink() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"nested").unwrap();
    memory.write(96, b"shortcut").unwrap();
    memory.write(128, b"../target.bin").unwrap();
    memory.write(512, &[0xa5; 64]).unwrap();

    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);
    assert_eq!(errno(&mut vm, "mkdir", &path_args(3, 64, 6)), ERRNO_SUCCESS);

    let full = RIGHTS_FD_READDIR | RIGHTS_PATH_READLINK | RIGHTS_PATH_SYMLINK;
    assert_eq!(
        errno(&mut vm, "open", &open_args(3, 64, 6, full, 300)),
        ERRNO_SUCCESS
    );
    let full_fd = read_u32(&memory, 300);
    assert_eq!(
        errno(&mut vm, "symlink", &symlink_args(128, 13, full_fd, 96, 8),),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(
            &mut vm,
            "readlink",
            &readlink_args(full_fd, 96, 8, 512, 64, 304),
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u32(&memory, 304), 13);
    assert_eq!(memory.read(512, 13).unwrap(), b"../target.bin");

    let read_only = RIGHTS_FD_READDIR | RIGHTS_PATH_READLINK;
    assert_eq!(
        errno(&mut vm, "open", &open_args(3, 64, 6, read_only, 308)),
        ERRNO_SUCCESS
    );
    let read_only_fd = read_u32(&memory, 308);
    memory.write(160, b"blocked").unwrap();
    assert_eq!(
        errno(
            &mut vm,
            "symlink",
            &symlink_args(128, 13, read_only_fd, 160, 7),
        ),
        ERRNO_NOTCAPABLE
    );
    assert_eq!(
        errno(
            &mut vm,
            "readlink",
            &readlink_args(read_only_fd, 96, 8, 544, 32, 312),
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u32(&memory, 312), 13);
    assert_eq!(memory.read(544, 13).unwrap(), b"../target.bin");
}
