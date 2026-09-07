use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, Value};
use wasm_wasi::{
    WasiPreview1, ERRNO_BADF, ERRNO_FAULT, ERRNO_INVAL, ERRNO_NOTCAPABLE, ERRNO_OVERFLOW,
    ERRNO_SUCCESS, RIGHTS_FD_READ, RIGHTS_FD_SEEK, RIGHTS_FD_TELL,
};

const WHENCE_SET: i32 = 0;
const WHENCE_CUR: i32 = 1;
const WHENCE_END: i32 = 2;

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
    function_type(&mut types, &[0x7f, 0x7e, 0x7f, 0x7f]);
    function_type(&mut types, &[0x7f, 0x7f]);
    section(&mut module, 1, &types);

    let mut imports = vec![5];
    add_function_import(&mut imports, "path_open", 0);
    add_function_import(&mut imports, "fd_read", 1);
    add_function_import(&mut imports, "fd_seek", 2);
    add_function_import(&mut imports, "fd_tell", 3);
    name(&mut imports, "env");
    name(&mut imports, "memory");
    imports.extend([2, 0, 1]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[4, 0, 1, 2, 3]);

    let mut exports = vec![4];
    for (export_name, function_index) in [("open", 4), ("read", 5), ("seek", 6), ("tell", 7)] {
        name(&mut exports, export_name);
        exports.push(0);
        u32leb(&mut exports, function_index);
    }
    section(&mut module, 7, &exports);

    let bodies = [
        forwarder(9, 0),
        forwarder(4, 1),
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

fn read_args(fd: u32) -> [Value; 4] {
    [
        Value::I32(fd as i32),
        Value::I32(128),
        Value::I32(1),
        Value::I32(160),
    ]
}

fn seek_args(fd: u32, delta: i64, whence: i32, output: i32) -> [Value; 4] {
    [
        Value::I32(fd as i32),
        Value::I64(delta),
        Value::I32(whence),
        Value::I32(output),
    ]
}

fn tell_args(fd: u32, output: i32) -> [Value; 2] {
    [Value::I32(fd as i32), Value::I32(output)]
}

fn configured_wasi() -> WasiPreview1 {
    WasiPreview1::new()
        .with_preopen("/sandbox")
        .unwrap()
        .with_read_only_file("/sandbox", "docs/hello.txt", b"abcdef")
        .unwrap()
}

fn prepare_memory(memory: &MemoryHandle, destination: u32, length: u32) {
    memory.write(64, b"docs/hello.txt").unwrap();
    memory.write(128, &destination.to_le_bytes()).unwrap();
    memory.write(132, &length.to_le_bytes()).unwrap();
}

fn opened_fd(vm: &mut Instance, memory: &MemoryHandle, rights: u64) -> u32 {
    assert_eq!(errno(vm, "open", &open_args(rights)), ERRNO_SUCCESS);
    u32::from_le_bytes(memory.read(100, 4).unwrap().try_into().unwrap())
}

fn read_u64(memory: &MemoryHandle, address: u32) -> u64 {
    u64::from_le_bytes(memory.read(address, 8).unwrap().try_into().unwrap())
}

fn write_u64(memory: &MemoryHandle, address: u32, value: u64) {
    memory.write(address, &value.to_le_bytes()).unwrap();
}

#[test]
fn seek_tell_and_read_share_one_descriptor_cursor() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    prepare_memory(&memory, 256, 2);
    let wasi = configured_wasi();
    let mut vm = instantiate(&memory, &wasi);
    let fd = opened_fd(&mut vm, &memory, RIGHTS_FD_READ | RIGHTS_FD_SEEK);

    assert_eq!(errno(&mut vm, "read", &read_args(fd)), ERRNO_SUCCESS);
    assert_eq!(memory.read(256, 2).unwrap(), b"ab");
    assert_eq!(
        errno(&mut vm, "tell", &tell_args(fd, 192)),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u64(&memory, 192), 2);

    assert_eq!(
        errno(&mut vm, "seek", &seek_args(fd, 4, WHENCE_SET, 200)),
        ERRNO_SUCCESS
    );
    memory.write(128, &258u32.to_le_bytes()).unwrap();
    assert_eq!(errno(&mut vm, "read", &read_args(fd)), ERRNO_SUCCESS);
    assert_eq!(memory.read(258, 2).unwrap(), b"ef");

    assert_eq!(
        errno(&mut vm, "seek", &seek_args(fd, -3, WHENCE_CUR, 200)),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u64(&memory, 200), 3);
    memory.write(128, &260u32.to_le_bytes()).unwrap();
    assert_eq!(errno(&mut vm, "read", &read_args(fd)), ERRNO_SUCCESS);
    assert_eq!(memory.read(260, 2).unwrap(), b"de");

    assert_eq!(
        errno(&mut vm, "seek", &seek_args(fd, -1, WHENCE_END, 200)),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u64(&memory, 200), 5);
    memory.write(128, &262u32.to_le_bytes()).unwrap();
    assert_eq!(errno(&mut vm, "read", &read_args(fd)), ERRNO_SUCCESS);
    assert_eq!(memory.read(262, 1).unwrap(), b"f");

    assert_eq!(
        errno(&mut vm, "seek", &seek_args(fd, 10, WHENCE_SET, 200)),
        ERRNO_SUCCESS
    );
    memory.write(128, &264u32.to_le_bytes()).unwrap();
    memory.write(132, &2u32.to_le_bytes()).unwrap();
    memory.write(264, &[0xaa; 2]).unwrap();
    assert_eq!(errno(&mut vm, "read", &read_args(fd)), ERRNO_SUCCESS);
    assert_eq!(
        u32::from_le_bytes(memory.read(160, 4).unwrap().try_into().unwrap()),
        0
    );
    assert_eq!(memory.read(264, 2).unwrap(), vec![0xaa; 2]);
    assert_eq!(
        errno(&mut vm, "tell", &tell_args(fd, 192)),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u64(&memory, 192), 10);
}

#[test]
fn seek_right_implies_tell_and_tell_only_cannot_move_the_cursor() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    prepare_memory(&memory, 256, 1);
    let wasi = configured_wasi();
    let mut vm = instantiate(&memory, &wasi);

    let seek_fd = opened_fd(&mut vm, &memory, RIGHTS_FD_SEEK);
    assert_eq!(
        errno(&mut vm, "tell", &tell_args(seek_fd, 192)),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u64(&memory, 192), 0);

    let tell_fd = opened_fd(&mut vm, &memory, RIGHTS_FD_READ | RIGHTS_FD_TELL);
    assert_eq!(
        errno(&mut vm, "tell", &tell_args(tell_fd, 192)),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(&mut vm, "seek", &seek_args(tell_fd, 0, WHENCE_CUR, 200)),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u64(&memory, 200), 0);

    write_u64(&memory, 200, 0xcccccccccccccccc);
    assert_eq!(
        errno(&mut vm, "seek", &seek_args(tell_fd, 1, WHENCE_CUR, 200)),
        ERRNO_NOTCAPABLE
    );
    assert_eq!(read_u64(&memory, 200), 0xcccccccccccccccc);
    assert_eq!(
        errno(&mut vm, "seek", &seek_args(tell_fd, 0, WHENCE_SET, 200)),
        ERRNO_NOTCAPABLE
    );
    assert_eq!(
        errno(&mut vm, "tell", &tell_args(tell_fd, 192)),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u64(&memory, 192), 0);
}

#[test]
fn rejected_seek_calls_leave_cursor_and_output_unchanged() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    prepare_memory(&memory, 256, 2);
    let wasi = configured_wasi();
    let mut vm = instantiate(&memory, &wasi);
    let fd = opened_fd(&mut vm, &memory, RIGHTS_FD_READ | RIGHTS_FD_SEEK);

    assert_eq!(errno(&mut vm, "read", &read_args(fd)), ERRNO_SUCCESS);
    assert_eq!(
        errno(&mut vm, "tell", &tell_args(fd, 192)),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u64(&memory, 192), 2);

    write_u64(&memory, 200, 0xdeadbeefdeadbeef);
    assert_eq!(
        errno(&mut vm, "seek", &seek_args(fd, -3, WHENCE_CUR, 200)),
        ERRNO_INVAL
    );
    assert_eq!(read_u64(&memory, 200), 0xdeadbeefdeadbeef);
    assert_eq!(
        errno(&mut vm, "seek", &seek_args(fd, 0, 99, 200)),
        ERRNO_INVAL
    );
    assert_eq!(read_u64(&memory, 200), 0xdeadbeefdeadbeef);

    assert_eq!(
        errno(&mut vm, "seek", &seek_args(fd, 4, WHENCE_SET, 65_532)),
        ERRNO_FAULT
    );
    assert_eq!(
        errno(&mut vm, "tell", &tell_args(fd, 192)),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u64(&memory, 192), 2);

    assert_eq!(
        errno(&mut vm, "seek", &seek_args(fd, i64::MAX, WHENCE_SET, 200)),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(&mut vm, "seek", &seek_args(fd, i64::MAX, WHENCE_CUR, 200)),
        ERRNO_SUCCESS
    );
    write_u64(&memory, 200, 0xaaaaaaaaaaaaaaaa);
    assert_eq!(
        errno(&mut vm, "seek", &seek_args(fd, 2, WHENCE_CUR, 200)),
        ERRNO_OVERFLOW
    );
    assert_eq!(read_u64(&memory, 200), 0xaaaaaaaaaaaaaaaa);
    assert_eq!(
        errno(&mut vm, "tell", &tell_args(fd, 192)),
        ERRNO_SUCCESS
    );
    assert_eq!(read_u64(&memory, 192), u64::MAX - 1);

    write_u64(&memory, 200, 0xbbbbbbbbbbbbbbbb);
    assert_eq!(
        errno(&mut vm, "seek", &seek_args(99, 0, WHENCE_SET, 200)),
        ERRNO_BADF
    );
    assert_eq!(read_u64(&memory, 200), 0xbbbbbbbbbbbbbbbb);
}

#[test]
fn descriptors_without_position_rights_fail_closed() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    prepare_memory(&memory, 256, 1);
    let wasi = configured_wasi();
    let mut vm = instantiate(&memory, &wasi);
    let fd = opened_fd(&mut vm, &memory, RIGHTS_FD_READ);

    write_u64(&memory, 192, 0x1111111111111111);
    write_u64(&memory, 200, 0x2222222222222222);
    assert_eq!(
        errno(&mut vm, "tell", &tell_args(fd, 192)),
        ERRNO_NOTCAPABLE
    );
    assert_eq!(
        errno(&mut vm, "seek", &seek_args(fd, 0, WHENCE_CUR, 200)),
        ERRNO_NOTCAPABLE
    );
    assert_eq!(read_u64(&memory, 192), 0x1111111111111111);
    assert_eq!(read_u64(&memory, 200), 0x2222222222222222);
}
