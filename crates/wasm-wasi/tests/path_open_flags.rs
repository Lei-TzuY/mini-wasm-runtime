use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, Value};
use wasm_wasi::{
    WasiPreview1, ERRNO_EXIST, ERRNO_FAULT, ERRNO_NOTCAPABLE, ERRNO_SUCCESS, OFLAGS_CREAT,
    OFLAGS_EXCL, OFLAGS_TRUNC, RIGHTS_FD_READ, RIGHTS_FD_WRITE,
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

    let mut types = vec![2];
    function_type(
        &mut types,
        &[0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7e, 0x7e, 0x7f, 0x7f],
    );
    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7f]);
    section(&mut module, 1, &types);

    let mut imports = vec![3];
    add_function_import(&mut imports, "path_open", 0);
    add_function_import(&mut imports, "fd_read", 1);
    name(&mut imports, "env");
    name(&mut imports, "memory");
    imports.extend([2, 0, 1]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[2, 0, 1]);

    let mut exports = vec![2];
    for (export_name, function_index) in [("open", 2), ("read", 3)] {
        name(&mut exports, export_name);
        exports.push(0);
        u32leb(&mut exports, function_index);
    }
    section(&mut module, 7, &exports);

    let bodies = [forwarder(9, 0), forwarder(4, 1)];
    let mut code = vec![2];
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

#[test]
fn create_exclusive_is_atomic_and_only_creates_missing_paths() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"seed.bin").unwrap();
    memory.write(80, b"new.bin").unwrap();
    memory.write(100, &0xdead_beefu32.to_le_bytes()).unwrap();
    memory.write(104, &0xfeed_faceu32.to_le_bytes()).unwrap();

    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap()
        .with_writable_file("/sandbox", "seed.bin", b"abc")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);

    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(64, 8, OFLAGS_CREAT | OFLAGS_EXCL, RIGHTS_FD_READ, 100),
        ),
        ERRNO_EXIST
    );
    assert_eq!(
        u32::from_le_bytes(memory.read(100, 4).unwrap().try_into().unwrap()),
        0xdead_beef
    );
    assert_eq!(wasi.file_snapshot("/sandbox", "seed.bin").unwrap(), b"abc");

    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(80, 7, OFLAGS_CREAT | OFLAGS_EXCL, RIGHTS_FD_READ, 100),
        ),
        ERRNO_SUCCESS
    );
    assert!(wasi.file_snapshot("/sandbox", "new.bin").is_some());

    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(80, 7, OFLAGS_CREAT | OFLAGS_EXCL, RIGHTS_FD_READ, 104),
        ),
        ERRNO_EXIST
    );
    assert_eq!(
        u32::from_le_bytes(memory.read(104, 4).unwrap().try_into().unwrap()),
        0xfeed_face
    );
}

#[test]
fn truncation_is_visible_through_already_open_shared_backing() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"seed.bin").unwrap();
    memory.write(128, &256u32.to_le_bytes()).unwrap();
    memory.write(132, &8u32.to_le_bytes()).unwrap();

    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap()
        .with_writable_file("/sandbox", "seed.bin", b"abcdef")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);

    assert_eq!(
        errno(&mut vm, "open", &open_args(64, 8, 0, RIGHTS_FD_READ, 100)),
        ERRNO_SUCCESS
    );
    let old_fd = u32::from_le_bytes(memory.read(100, 4).unwrap().try_into().unwrap());

    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(64, 8, OFLAGS_TRUNC, RIGHTS_FD_READ | RIGHTS_FD_WRITE, 104,),
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(wasi.file_snapshot("/sandbox", "seed.bin").unwrap(), b"");

    memory.write(136, &0xdead_beefu32.to_le_bytes()).unwrap();
    assert_eq!(
        errno(
            &mut vm,
            "read",
            &[
                Value::I32(old_fd as i32),
                Value::I32(128),
                Value::I32(1),
                Value::I32(136),
            ],
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        u32::from_le_bytes(memory.read(136, 4).unwrap().try_into().unwrap()),
        0
    );
}

#[test]
fn truncation_requires_write_capability_and_preflights_output_before_mutation() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"seed.bin").unwrap();
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap()
        .with_writable_file("/sandbox", "seed.bin", b"abc")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);

    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(64, 8, OFLAGS_TRUNC, RIGHTS_FD_READ, 100),
        ),
        ERRNO_NOTCAPABLE
    );
    assert_eq!(wasi.file_snapshot("/sandbox", "seed.bin").unwrap(), b"abc");

    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(
                64,
                8,
                OFLAGS_TRUNC,
                RIGHTS_FD_READ | RIGHTS_FD_WRITE,
                65_534,
            ),
        ),
        ERRNO_FAULT
    );
    assert_eq!(wasi.file_snapshot("/sandbox", "seed.bin").unwrap(), b"abc");

    let ro_memory = MemoryHandle::new(1, Some(1)).unwrap();
    ro_memory.write(64, b"seed.bin").unwrap();
    let readonly = WasiPreview1::new()
        .with_preopen("/ro")
        .unwrap()
        .with_read_only_file("/ro", "seed.bin", b"abc")
        .unwrap();
    let mut ro_vm = instantiate(&ro_memory, &readonly);
    assert_eq!(
        errno(
            &mut ro_vm,
            "open",
            &open_args(64, 8, OFLAGS_TRUNC, RIGHTS_FD_READ | RIGHTS_FD_WRITE, 100,),
        ),
        ERRNO_NOTCAPABLE
    );
    assert_eq!(readonly.file_snapshot("/ro", "seed.bin").unwrap(), b"abc");
}
