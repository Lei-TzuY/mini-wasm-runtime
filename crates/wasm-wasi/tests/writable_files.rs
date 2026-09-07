use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, Value};
use wasm_wasi::{
    WasiFilesystemError, WasiPreview1, ERRNO_FAULT, ERRNO_FBIG, ERRNO_NOTCAPABLE, ERRNO_SUCCESS,
    OFLAGS_CREAT, RIGHTS_FD_READ, RIGHTS_FD_SEEK, RIGHTS_FD_TELL, RIGHTS_FD_WRITE,
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

    let mut types = vec![5];
    function_type(
        &mut types,
        &[0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7e, 0x7e, 0x7f, 0x7f],
    );
    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7e, 0x7f]);
    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7f]);
    function_type(&mut types, &[0x7f, 0x7f]);
    function_type(&mut types, &[0x7f]);
    section(&mut module, 1, &types);

    let mut imports = vec![6];
    add_function_import(&mut imports, "path_open", 0);
    add_function_import(&mut imports, "fd_pwrite", 1);
    add_function_import(&mut imports, "fd_read", 2);
    add_function_import(&mut imports, "fd_tell", 3);
    add_function_import(&mut imports, "fd_close", 4);
    name(&mut imports, "env");
    name(&mut imports, "memory");
    imports.extend([2, 0, 1]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[5, 0, 1, 2, 3, 4]);

    let mut exports = vec![5];
    for (export_name, function_index) in [
        ("open", 5),
        ("pwrite", 6),
        ("read", 7),
        ("tell", 8),
        ("close", 9),
    ] {
        name(&mut exports, export_name);
        exports.push(0);
        u32leb(&mut exports, function_index);
    }
    section(&mut module, 7, &exports);

    let bodies = [
        forwarder(9, 0),
        forwarder(5, 1),
        forwarder(4, 2),
        forwarder(2, 3),
        forwarder(1, 4),
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

fn pwrite_args(fd: u32, offset: u64, nwritten_ptr: i32) -> [Value; 5] {
    [
        Value::I32(fd as i32),
        Value::I32(128),
        Value::I32(1),
        Value::I64(offset as i64),
        Value::I32(nwritten_ptr),
    ]
}

fn read_args(fd: u32) -> [Value; 4] {
    [
        Value::I32(fd as i32),
        Value::I32(176),
        Value::I32(1),
        Value::I32(184),
    ]
}

#[test]
fn writable_preopen_creates_sparse_file_and_pwrite_preserves_descriptor_cursor() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"created.bin").unwrap();
    memory.write(128, &256u32.to_le_bytes()).unwrap();
    memory.write(132, &3u32.to_le_bytes()).unwrap();
    memory.write(256, b"xyz").unwrap();
    memory.write(176, &300u32.to_le_bytes()).unwrap();
    memory.write(180, &6u32.to_le_bytes()).unwrap();

    let wasi = WasiPreview1::new()
        .with_writable_preopen("/scratch")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);
    let rights = RIGHTS_FD_READ | RIGHTS_FD_WRITE | RIGHTS_FD_SEEK | RIGHTS_FD_TELL;

    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(3, 64, 11, OFLAGS_CREAT, rights, 100),
        ),
        ERRNO_SUCCESS
    );
    let fd = u32::from_le_bytes(memory.read(100, 4).unwrap().try_into().unwrap());
    assert_eq!(fd, 4);

    assert_eq!(
        errno(&mut vm, "pwrite", &pwrite_args(fd, 3, 160)),
        ERRNO_SUCCESS
    );
    assert_eq!(
        u32::from_le_bytes(memory.read(160, 4).unwrap().try_into().unwrap()),
        3
    );
    assert_eq!(
        errno(
            &mut vm,
            "tell",
            &[Value::I32(fd as i32), Value::I32(168)],
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        u64::from_le_bytes(memory.read(168, 8).unwrap().try_into().unwrap()),
        0
    );

    assert_eq!(errno(&mut vm, "read", &read_args(fd)), ERRNO_SUCCESS);
    assert_eq!(memory.read(300, 6).unwrap(), b"\0\0\0xyz");
    assert_eq!(
        u32::from_le_bytes(memory.read(184, 4).unwrap().try_into().unwrap()),
        6
    );
    assert_eq!(
        errno(&mut vm, "close", &[Value::I32(fd as i32)]),
        ERRNO_SUCCESS
    );
    assert_eq!(
        wasi.file_snapshot("/scratch", "created.bin").unwrap(),
        b"\0\0\0xyz"
    );
}

#[test]
fn writable_policy_and_pwrite_fail_closed_without_mutating_file_or_results() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"seed.bin").unwrap();
    memory.write(128, &256u32.to_le_bytes()).unwrap();
    memory.write(132, &1u32.to_le_bytes()).unwrap();
    memory.write(256, b"Z").unwrap();

    let wasi = WasiPreview1::new()
        .with_writable_preopen("/scratch")
        .unwrap()
        .with_writable_file("/scratch", "seed.bin", b"abc")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);

    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(
                3,
                64,
                8,
                0,
                RIGHTS_FD_READ | RIGHTS_FD_WRITE | RIGHTS_FD_SEEK,
                100,
            ),
        ),
        ERRNO_SUCCESS
    );
    let fd = u32::from_le_bytes(memory.read(100, 4).unwrap().try_into().unwrap());

    memory.write(160, &0xdeadbeefu32.to_le_bytes()).unwrap();
    assert_eq!(
        errno(
            &mut vm,
            "pwrite",
            &pwrite_args(fd, 16 * 1024 * 1024, 160),
        ),
        ERRNO_FBIG
    );
    assert_eq!(
        u32::from_le_bytes(memory.read(160, 4).unwrap().try_into().unwrap()),
        0xdeadbeef
    );
    assert_eq!(wasi.file_snapshot("/scratch", "seed.bin").unwrap(), b"abc");

    assert_eq!(
        errno(&mut vm, "pwrite", &pwrite_args(fd, 1, 65_534)),
        ERRNO_FAULT
    );
    assert_eq!(wasi.file_snapshot("/scratch", "seed.bin").unwrap(), b"abc");

    let readonly = WasiPreview1::new().with_preopen("/ro").unwrap();
    assert!(matches!(
        readonly
            .clone()
            .with_writable_file("/ro", "forbidden.bin", b"x"),
        Err(WasiFilesystemError::ReadOnlyPreopen)
    ));

    let ro_memory = MemoryHandle::new(1, Some(1)).unwrap();
    ro_memory.write(64, b"new.bin").unwrap();
    ro_memory.write(100, &0xfeedfaceu32.to_le_bytes()).unwrap();
    let mut ro_vm = instantiate(&ro_memory, &readonly);
    assert_eq!(
        errno(
            &mut ro_vm,
            "open",
            &open_args(
                3,
                64,
                7,
                OFLAGS_CREAT,
                RIGHTS_FD_READ | RIGHTS_FD_WRITE | RIGHTS_FD_SEEK,
                100,
            ),
        ),
        ERRNO_NOTCAPABLE
    );
    assert_eq!(
        u32::from_le_bytes(ro_memory.read(100, 4).unwrap().try_into().unwrap()),
        0xfeedface
    );
    assert!(readonly.file_snapshot("/ro", "new.bin").is_none());
}
