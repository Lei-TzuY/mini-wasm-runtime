use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, Value};
use wasm_wasi::{
    WasiFilesystemError, WasiPreview1, ERRNO_BADF, ERRNO_FAULT, ERRNO_NOENT, ERRNO_NOTCAPABLE,
    ERRNO_SUCCESS, FILETYPE_REGULAR_FILE, RIGHTS_FD_READ, RIGHTS_FD_WRITE,
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
    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7f]);
    function_type(&mut types, &[0x7f]);
    function_type(&mut types, &[0x7f, 0x7f]);
    section(&mut module, 1, &types);

    let mut imports = vec![5];
    add_function_import(&mut imports, "path_open", 0);
    add_function_import(&mut imports, "fd_read", 1);
    add_function_import(&mut imports, "fd_close", 2);
    add_function_import(&mut imports, "fd_fdstat_get", 3);
    name(&mut imports, "env");
    name(&mut imports, "memory");
    imports.extend([2, 0, 1]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[4, 0, 1, 2, 3]);

    let mut exports = vec![4];
    for (export_name, function_index) in [("open", 4), ("read", 5), ("close", 6), ("stat", 7)] {
        name(&mut exports, export_name);
        exports.push(0);
        u32leb(&mut exports, function_index);
    }
    section(&mut module, 7, &exports);

    let bodies = [
        forwarder(9, 0),
        forwarder(4, 1),
        forwarder(1, 2),
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

fn open_args(path_ptr: i32, path_len: i32, rights: u64, opened_fd_ptr: i32) -> Vec<Value> {
    vec![
        Value::I32(3),
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

fn read_args(fd: u32) -> [Value; 4] {
    [
        Value::I32(fd as i32),
        Value::I32(128),
        Value::I32(1),
        Value::I32(160),
    ]
}

fn configured_wasi() -> WasiPreview1 {
    WasiPreview1::new()
        .with_preopen("/sandbox")
        .unwrap()
        .with_read_only_file("/sandbox", "docs/hello.txt", b"abcdef")
        .unwrap()
}

#[test]
fn path_open_read_stat_and_close_form_one_descriptor_lifecycle() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"docs/hello.txt").unwrap();
    memory.write(128, &256u32.to_le_bytes()).unwrap();
    memory.write(132, &3u32.to_le_bytes()).unwrap();
    let wasi = configured_wasi();
    let mut vm = instantiate(&memory, &wasi);

    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(64, 14, RIGHTS_FD_READ, 100)
        ),
        ERRNO_SUCCESS
    );
    let fd = u32::from_le_bytes(memory.read(100, 4).unwrap().try_into().unwrap());
    assert_eq!(fd, 4);

    assert_eq!(
        errno(
            &mut vm,
            "stat",
            &[Value::I32(fd as i32), Value::I32(192)]
        ),
        ERRNO_SUCCESS
    );
    let fdstat = memory.read(192, 24).unwrap();
    assert_eq!(fdstat[0], FILETYPE_REGULAR_FILE);
    assert_eq!(
        u64::from_le_bytes(fdstat[8..16].try_into().unwrap()),
        RIGHTS_FD_READ
    );
    assert_eq!(u64::from_le_bytes(fdstat[16..24].try_into().unwrap()), 0);

    assert_eq!(errno(&mut vm, "read", &read_args(fd)), ERRNO_SUCCESS);
    assert_eq!(memory.read(256, 3).unwrap(), b"abc");
    assert_eq!(
        u32::from_le_bytes(memory.read(160, 4).unwrap().try_into().unwrap()),
        3
    );

    memory.write(128, &259u32.to_le_bytes()).unwrap();
    memory.write(132, &8u32.to_le_bytes()).unwrap();
    assert_eq!(errno(&mut vm, "read", &read_args(fd)), ERRNO_SUCCESS);
    assert_eq!(memory.read(259, 3).unwrap(), b"def");
    assert_eq!(
        u32::from_le_bytes(memory.read(160, 4).unwrap().try_into().unwrap()),
        3
    );

    memory.write(262, &[0xaa; 4]).unwrap();
    memory.write(128, &262u32.to_le_bytes()).unwrap();
    memory.write(132, &4u32.to_le_bytes()).unwrap();
    assert_eq!(errno(&mut vm, "read", &read_args(fd)), ERRNO_SUCCESS);
    assert_eq!(
        u32::from_le_bytes(memory.read(160, 4).unwrap().try_into().unwrap()),
        0
    );
    assert_eq!(memory.read(262, 4).unwrap(), vec![0xaa; 4]);

    assert_eq!(
        errno(&mut vm, "close", &[Value::I32(fd as i32)]),
        ERRNO_SUCCESS
    );
    memory.write(192, &[0xbb; 24]).unwrap();
    assert_eq!(
        errno(
            &mut vm,
            "stat",
            &[Value::I32(fd as i32), Value::I32(192)]
        ),
        ERRNO_BADF
    );
    assert_eq!(memory.read(192, 24).unwrap(), vec![0xbb; 24]);

    memory.write(160, &0xccccccccu32.to_le_bytes()).unwrap();
    assert_eq!(errno(&mut vm, "read", &read_args(fd)), ERRNO_BADF);
    assert_eq!(
        u32::from_le_bytes(memory.read(160, 4).unwrap().try_into().unwrap()),
        0xcccccccc
    );
}

#[test]
fn rejected_path_open_calls_do_not_allocate_or_mutate_the_output_fd() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let wasi = configured_wasi();
    let mut vm = instantiate(&memory, &wasi);
    memory.write(100, &0xdeadbeefu32.to_le_bytes()).unwrap();

    memory.write(64, b"../secret").unwrap();
    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(64, 9, RIGHTS_FD_READ, 100)
        ),
        ERRNO_NOTCAPABLE
    );
    assert_eq!(
        u32::from_le_bytes(memory.read(100, 4).unwrap().try_into().unwrap()),
        0xdeadbeef
    );

    memory.write(64, b"docs/missing").unwrap();
    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(64, 12, RIGHTS_FD_READ, 100)
        ),
        ERRNO_NOENT
    );
    assert_eq!(
        u32::from_le_bytes(memory.read(100, 4).unwrap().try_into().unwrap()),
        0xdeadbeef
    );

    memory.write(64, b"docs/hello.txt").unwrap();
    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(64, 14, RIGHTS_FD_WRITE, 100)
        ),
        ERRNO_NOTCAPABLE
    );
    assert_eq!(
        u32::from_le_bytes(memory.read(100, 4).unwrap().try_into().unwrap()),
        0xdeadbeef
    );

    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(64, 14, RIGHTS_FD_READ, 65_534)
        ),
        ERRNO_FAULT
    );

    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(64, 14, RIGHTS_FD_READ, 100)
        ),
        ERRNO_SUCCESS
    );
    let fd = u32::from_le_bytes(memory.read(100, 4).unwrap().try_into().unwrap());
    assert_eq!(fd, 4);
    assert_eq!(
        errno(&mut vm, "close", &[Value::I32(fd as i32)]),
        ERRNO_SUCCESS
    );

    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(64, 14, RIGHTS_FD_READ, 100)
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        u32::from_le_bytes(memory.read(100, 4).unwrap().try_into().unwrap()),
        4
    );
}

#[test]
fn child_descriptor_rights_are_attenuated_by_path_open_request() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"docs/hello.txt").unwrap();
    memory.write(128, &256u32.to_le_bytes()).unwrap();
    memory.write(132, &3u32.to_le_bytes()).unwrap();
    let wasi = configured_wasi();
    let mut vm = instantiate(&memory, &wasi);

    assert_eq!(
        errno(&mut vm, "open", &open_args(64, 14, 0, 100)),
        ERRNO_SUCCESS
    );
    let fd = u32::from_le_bytes(memory.read(100, 4).unwrap().try_into().unwrap());
    assert_eq!(fd, 4);

    assert_eq!(errno(&mut vm, "read", &read_args(fd)), ERRNO_NOTCAPABLE);
    assert_eq!(
        errno(
            &mut vm,
            "stat",
            &[Value::I32(fd as i32), Value::I32(192)]
        ),
        ERRNO_SUCCESS
    );
    let fdstat = memory.read(192, 24).unwrap();
    assert_eq!(u64::from_le_bytes(fdstat[8..16].try_into().unwrap()), 0);
}

#[test]
fn mounted_file_configuration_requires_existing_preopen_and_safe_relative_path() {
    assert!(matches!(
        WasiPreview1::new().with_read_only_file("/missing", "file.txt", b"x"),
        Err(WasiFilesystemError::UnknownPreopen { .. })
    ));

    let wasi = WasiPreview1::new().with_preopen("/sandbox").unwrap();
    assert!(matches!(
        wasi.clone().with_read_only_file("/sandbox", "", b"x"),
        Err(WasiFilesystemError::EmptyRelativePath)
    ));
    for path in [
        "/absolute",
        "../escape",
        "a/../b",
        "a//b",
        "a/./b",
        "trailing/",
    ] {
        assert!(matches!(
            wasi.clone().with_read_only_file("/sandbox", path, b"x"),
            Err(WasiFilesystemError::UnsafeRelativePath)
        ));
    }

    let duplicate = wasi
        .with_read_only_file("/sandbox", "file.txt", b"one")
        .unwrap();
    assert!(matches!(
        duplicate.with_read_only_file("/sandbox", "file.txt", b"two"),
        Err(WasiFilesystemError::DuplicateFile)
    ));
}
