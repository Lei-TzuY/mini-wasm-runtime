use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, Value};
use wasm_wasi::{
    WasiClockId, WasiPreview1, ERRNO_INVAL, ERRNO_NOTCAPABLE, ERRNO_SUCCESS,
    RIGHTS_FD_FILESTAT_GET, RIGHTS_PATH_FILESTAT_GET,
};

const RIGHTS_PATH_FILESTAT_SET_TIMES: u64 = 1 << 20;
const RIGHTS_FD_FILESTAT_SET_TIMES: u64 = 1 << 23;
const FSTFLAGS_ATIM: u32 = 1 << 0;
const FSTFLAGS_ATIM_NOW: u32 = 1 << 1;
const FSTFLAGS_MTIM: u32 = 1 << 2;
const FSTFLAGS_MTIM_NOW: u32 = 1 << 3;

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
    function_type(&mut types, &[0x7f, 0x7f]);
    function_type(&mut types, &[0x7f, 0x7e, 0x7e, 0x7f]);
    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7f, 0x7f]);
    function_type(
        &mut types,
        &[0x7f, 0x7f, 0x7f, 0x7f, 0x7e, 0x7e, 0x7f],
    );
    section(&mut module, 1, &types);

    let mut imports = vec![6];
    add_function_import(&mut imports, "path_open", 0);
    add_function_import(&mut imports, "fd_filestat_get", 1);
    add_function_import(&mut imports, "fd_filestat_set_times", 2);
    add_function_import(&mut imports, "path_filestat_get", 3);
    add_function_import(&mut imports, "path_filestat_set_times", 4);
    name(&mut imports, "env");
    name(&mut imports, "memory");
    imports.extend([2, 0, 1]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[5, 0, 1, 2, 3, 4]);

    let mut exports = vec![5];
    for (export_name, function_index) in [
        ("open", 5),
        ("fd_get", 6),
        ("fd_set", 7),
        ("path_get", 8),
        ("path_set", 9),
    ] {
        name(&mut exports, export_name);
        exports.push(0);
        u32leb(&mut exports, function_index);
    }
    section(&mut module, 7, &exports);

    let bodies = [
        forwarder(9, 0),
        forwarder(2, 1),
        forwarder(4, 2),
        forwarder(5, 3),
        forwarder(7, 4),
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

fn read_u32(memory: &MemoryHandle, ptr: u32) -> u32 {
    u32::from_le_bytes(memory.read(ptr, 4).unwrap().try_into().unwrap())
}

fn read_u64(memory: &MemoryHandle, ptr: u32) -> u64 {
    u64::from_le_bytes(memory.read(ptr, 8).unwrap().try_into().unwrap())
}

fn assert_times(memory: &MemoryHandle, filestat_ptr: u32, atim: u64, mtim: u64) {
    assert_eq!(read_u64(memory, filestat_ptr + 40), atim);
    assert_eq!(read_u64(memory, filestat_ptr + 48), mtim);
}

#[test]
fn descriptor_and_path_timestamp_updates_share_one_file_metadata_record() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"data.bin").unwrap();
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap()
        .with_writable_file("/sandbox", "data.bin", b"abc")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);
    let rights = RIGHTS_FD_FILESTAT_GET | RIGHTS_FD_FILESTAT_SET_TIMES;

    assert_eq!(
        errno(&mut vm, "open", &open_args(64, 8, rights, 100)),
        ERRNO_SUCCESS
    );
    let fd = read_u32(&memory, 100);

    assert_eq!(
        errno(
            &mut vm,
            "fd_set",
            &[
                Value::I32(fd as i32),
                Value::I64(11),
                Value::I64(22),
                Value::I32((FSTFLAGS_ATIM | FSTFLAGS_MTIM) as i32),
            ],
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(
            &mut vm,
            "fd_get",
            &[Value::I32(fd as i32), Value::I32(160)],
        ),
        ERRNO_SUCCESS
    );
    assert_times(&memory, 160, 11, 22);
    assert_eq!(
        errno(
            &mut vm,
            "path_get",
            &[
                Value::I32(3),
                Value::I32(0),
                Value::I32(64),
                Value::I32(8),
                Value::I32(240),
            ],
        ),
        ERRNO_SUCCESS
    );
    assert_times(&memory, 240, 11, 22);

    assert_eq!(
        errno(
            &mut vm,
            "path_set",
            &[
                Value::I32(3),
                Value::I32(0),
                Value::I32(64),
                Value::I32(8),
                Value::I64(33),
                Value::I64(44),
                Value::I32((FSTFLAGS_ATIM | FSTFLAGS_MTIM) as i32),
            ],
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(
            &mut vm,
            "fd_get",
            &[Value::I32(fd as i32), Value::I32(320)],
        ),
        ERRNO_SUCCESS
    );
    assert_times(&memory, 320, 33, 44);
}

#[test]
fn now_flags_use_the_injected_realtime_clock_without_ambient_time() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, b"data.bin").unwrap();
    let wasi = WasiPreview1::new()
        .with_clock(WasiClockId::Realtime, 1, 777)
        .with_writable_preopen("/sandbox")
        .unwrap()
        .with_writable_file("/sandbox", "data.bin", b"abc")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);
    let rights = RIGHTS_FD_FILESTAT_GET | RIGHTS_FD_FILESTAT_SET_TIMES;

    assert_eq!(
        errno(&mut vm, "open", &open_args(64, 8, rights, 100)),
        ERRNO_SUCCESS
    );
    let fd = read_u32(&memory, 100);
    assert_eq!(
        errno(
            &mut vm,
            "fd_set",
            &[
                Value::I32(fd as i32),
                Value::I64(123),
                Value::I64(456),
                Value::I32((FSTFLAGS_ATIM_NOW | FSTFLAGS_MTIM_NOW) as i32),
            ],
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(
            &mut vm,
            "fd_get",
            &[Value::I32(fd as i32), Value::I32(160)],
        ),
        ERRNO_SUCCESS
    );
    assert_times(&memory, 160, 777, 777);
}

#[test]
fn timestamp_mutation_rejects_missing_rights_conflicting_flags_and_missing_now_clock() {
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
            &open_args(64, 8, RIGHTS_FD_FILESTAT_GET, 100),
        ),
        ERRNO_SUCCESS
    );
    let fd = read_u32(&memory, 100);
    assert_eq!(
        errno(
            &mut vm,
            "fd_set",
            &[
                Value::I32(fd as i32),
                Value::I64(1),
                Value::I64(2),
                Value::I32(FSTFLAGS_ATIM as i32),
            ],
        ),
        ERRNO_NOTCAPABLE
    );

    assert_eq!(
        errno(
            &mut vm,
            "path_set",
            &[
                Value::I32(3),
                Value::I32(0),
                Value::I32(64),
                Value::I32(8),
                Value::I64(1),
                Value::I64(2),
                Value::I32((FSTFLAGS_ATIM | FSTFLAGS_ATIM_NOW) as i32),
            ],
        ),
        ERRNO_INVAL
    );
    assert_eq!(
        errno(
            &mut vm,
            "path_set",
            &[
                Value::I32(3),
                Value::I32(0),
                Value::I32(64),
                Value::I32(8),
                Value::I64(1),
                Value::I64(2),
                Value::I32(FSTFLAGS_MTIM_NOW as i32),
            ],
        ),
        ERRNO_INVAL
    );

    let _ = RIGHTS_PATH_FILESTAT_GET | RIGHTS_PATH_FILESTAT_SET_TIMES;
}
