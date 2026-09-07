use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, Value};
use wasm_wasi::{
    WasiPreview1, ERRNO_BADF, ERRNO_FAULT, ERRNO_INVAL, ERRNO_NOTCAPABLE, ERRNO_SUCCESS,
    RIGHTS_FD_READ, RIGHTS_FD_SEEK,
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
    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7e, 0x7f]);
    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7f]);
    function_type(&mut types, &[0x7f, 0x7f]);
    section(&mut module, 1, &types);

    let mut imports = vec![5];
    add_function_import(&mut imports, "path_open", 0);
    add_function_import(&mut imports, "fd_pread", 1);
    add_function_import(&mut imports, "fd_read", 2);
    add_function_import(&mut imports, "fd_tell", 3);
    name(&mut imports, "env");
    name(&mut imports, "memory");
    imports.extend([2, 0, 1]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[4, 0, 1, 2, 3]);

    let mut exports = vec![4];
    for (export_name, function_index) in [("open", 4), ("pread", 5), ("read", 6), ("tell", 7)] {
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

fn open_args(rights: u64) -> Vec<Value> {
    vec![
        Value::I32(3),
        Value::I32(0),
        Value::I32(64),
        Value::I32(14),
        Value::I32(0),
        Value::I64(rights as i64),
        Value::I64(0),
        Value::I32(0),
        Value::I32(100),
    ]
}

fn pread_args(fd: u32, iovs: u32, iovs_len: u32, offset: u64, nread: u32) -> [Value; 5] {
    [
        Value::I32(fd as i32),
        Value::I32(iovs as i32),
        Value::I32(iovs_len as i32),
        Value::I64(offset as i64),
        Value::I32(nread as i32),
    ]
}

fn read_args(fd: u32, iovs: u32, iovs_len: u32, nread: u32) -> [Value; 4] {
    [
        Value::I32(fd as i32),
        Value::I32(iovs as i32),
        Value::I32(iovs_len as i32),
        Value::I32(nread as i32),
    ]
}

fn tell_args(fd: u32, output: u32) -> [Value; 2] {
    [Value::I32(fd as i32), Value::I32(output as i32)]
}

fn configured_wasi() -> WasiPreview1 {
    WasiPreview1::new()
        .with_preopen("/sandbox")
        .unwrap()
        .with_read_only_file("/sandbox", "docs/hello.txt", b"abcdef")
        .unwrap()
}

fn prepare_memory(memory: &MemoryHandle) {
    memory.write(64, b"docs/hello.txt").unwrap();
    memory.write(128, &256u32.to_le_bytes()).unwrap();
    memory.write(132, &2u32.to_le_bytes()).unwrap();
    memory.write(136, &300u32.to_le_bytes()).unwrap();
    memory.write(140, &3u32.to_le_bytes()).unwrap();
    memory.write(144, &320u32.to_le_bytes()).unwrap();
    memory.write(148, &2u32.to_le_bytes()).unwrap();
}

fn opened_fd(vm: &mut Instance, memory: &MemoryHandle, rights: u64) -> u32 {
    assert_eq!(errno(vm, "open", &open_args(rights)), ERRNO_SUCCESS);
    u32::from_le_bytes(memory.read(100, 4).unwrap().try_into().unwrap())
}

fn read_u32(memory: &MemoryHandle, address: u32) -> u32 {
    u32::from_le_bytes(memory.read(address, 4).unwrap().try_into().unwrap())
}

fn read_u64(memory: &MemoryHandle, address: u32) -> u64 {
    u64::from_le_bytes(memory.read(address, 8).unwrap().try_into().unwrap())
}

#[test]
fn positioned_scatter_read_preserves_the_shared_descriptor_cursor() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    prepare_memory(&memory);
    let wasi = configured_wasi();
    let mut vm = instantiate(&memory, &wasi);
    let fd = opened_fd(&mut vm, &memory, RIGHTS_FD_READ | RIGHTS_FD_SEEK);

    assert_eq!(
        errno(&mut vm, "pread", &pread_args(fd, 128, 2, 1, 176)),
        ERRNO_SUCCESS
    );
    assert_eq!(memory.read(256, 2).unwrap(), b"bc");
    assert_eq!(memory.read(300, 3).unwrap(), b"def");
    assert_eq!(read_u32(&memory, 176), 5);
    assert_eq!(errno(&mut vm, "tell", &tell_args(fd, 184)), ERRNO_SUCCESS);
    assert_eq!(read_u64(&memory, 184), 0);

    assert_eq!(
        errno(&mut vm, "read", &read_args(fd, 144, 1, 188)),
        ERRNO_SUCCESS
    );
    assert_eq!(memory.read(320, 2).unwrap(), b"ab");
    assert_eq!(read_u32(&memory, 188), 2);
    assert_eq!(errno(&mut vm, "tell", &tell_args(fd, 184)), ERRNO_SUCCESS);
    assert_eq!(read_u64(&memory, 184), 2);

    memory.write(128, &322u32.to_le_bytes()).unwrap();
    memory.write(132, &4u32.to_le_bytes()).unwrap();
    memory.write(322, &[0xaa; 4]).unwrap();
    assert_eq!(
        errno(&mut vm, "pread", &pread_args(fd, 128, 1, 4, 176)),
        ERRNO_SUCCESS
    );
    assert_eq!(memory.read(322, 4).unwrap(), vec![b'e', b'f', 0xaa, 0xaa]);
    assert_eq!(read_u32(&memory, 176), 2);
    assert_eq!(errno(&mut vm, "tell", &tell_args(fd, 184)), ERRNO_SUCCESS);
    assert_eq!(read_u64(&memory, 184), 2);

    assert_eq!(
        errno(&mut vm, "pread", &pread_args(fd, 128, 1, u64::MAX, 176)),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u32(&memory, 176), 0);
    assert_eq!(errno(&mut vm, "tell", &tell_args(fd, 184)), ERRNO_SUCCESS);
    assert_eq!(read_u64(&memory, 184), 2);
}

#[test]
fn positioned_read_requires_both_read_and_seek_rights() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    prepare_memory(&memory);
    memory.write(176, &0x11223344u32.to_le_bytes()).unwrap();
    memory.write(256, &[0xcc; 2]).unwrap();
    let wasi = configured_wasi();
    let mut vm = instantiate(&memory, &wasi);

    let read_only = opened_fd(&mut vm, &memory, RIGHTS_FD_READ);
    assert_eq!(
        errno(&mut vm, "pread", &pread_args(read_only, 128, 1, 0, 176)),
        ERRNO_NOTCAPABLE
    );
    assert_eq!(read_u32(&memory, 176), 0x11223344);
    assert_eq!(memory.read(256, 2).unwrap(), vec![0xcc; 2]);

    let seek_only = opened_fd(&mut vm, &memory, RIGHTS_FD_SEEK);
    assert_eq!(
        errno(&mut vm, "pread", &pread_args(seek_only, 128, 1, 0, 176)),
        ERRNO_NOTCAPABLE
    );
    assert_eq!(read_u32(&memory, 176), 0x11223344);

    assert_eq!(
        errno(&mut vm, "pread", &pread_args(99, 128, 1, 0, 176)),
        ERRNO_BADF
    );
    assert_eq!(read_u32(&memory, 176), 0x11223344);
}

#[test]
fn positioned_read_preflights_guest_memory_and_reuses_read_limits() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    prepare_memory(&memory);
    memory.write(176, &0xaabbccddu32.to_le_bytes()).unwrap();
    let wasi = configured_wasi().with_read_limits(1, 4);
    let mut vm = instantiate(&memory, &wasi);
    let fd = opened_fd(&mut vm, &memory, RIGHTS_FD_READ | RIGHTS_FD_SEEK);

    assert_eq!(
        errno(&mut vm, "pread", &pread_args(fd, 128, 2, 0, 176)),
        ERRNO_INVAL
    );
    assert_eq!(read_u32(&memory, 176), 0xaabbccdd);
    assert_eq!(errno(&mut vm, "tell", &tell_args(fd, 184)), ERRNO_SUCCESS);
    assert_eq!(read_u64(&memory, 184), 0);

    memory.write(128, &65_534u32.to_le_bytes()).unwrap();
    memory.write(132, &4u32.to_le_bytes()).unwrap();
    assert_eq!(
        errno(&mut vm, "pread", &pread_args(fd, 128, 1, 0, 176)),
        ERRNO_FAULT
    );
    assert_eq!(read_u32(&memory, 176), 0xaabbccdd);
    assert_eq!(errno(&mut vm, "tell", &tell_args(fd, 184)), ERRNO_SUCCESS);
    assert_eq!(read_u64(&memory, 184), 0);

    memory.write(128, &256u32.to_le_bytes()).unwrap();
    memory.write(132, &2u32.to_le_bytes()).unwrap();
    memory.write(256, &[0xdd; 2]).unwrap();
    assert_eq!(
        errno(&mut vm, "pread", &pread_args(fd, 128, 1, 0, 65_534)),
        ERRNO_FAULT
    );
    assert_eq!(memory.read(256, 2).unwrap(), vec![0xdd; 2]);
    assert_eq!(errno(&mut vm, "tell", &tell_args(fd, 184)), ERRNO_SUCCESS);
    assert_eq!(read_u64(&memory, 184), 0);
}

#[test]
fn zero_length_positioned_read_is_side_effect_free() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    prepare_memory(&memory);
    memory.write(176, &0xfeedfaceu32.to_le_bytes()).unwrap();
    let wasi = configured_wasi();
    let mut vm = instantiate(&memory, &wasi);
    let fd = opened_fd(&mut vm, &memory, RIGHTS_FD_READ | RIGHTS_FD_SEEK);

    assert_eq!(
        errno(&mut vm, "pread", &pread_args(fd, 65_535, 0, 3, 176)),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u32(&memory, 176), 0);
    assert_eq!(errno(&mut vm, "tell", &tell_args(fd, 184)), ERRNO_SUCCESS);
    assert_eq!(read_u64(&memory, 184), 0);
}
