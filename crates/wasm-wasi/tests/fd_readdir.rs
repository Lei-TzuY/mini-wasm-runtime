use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, Value};
use wasm_wasi::{
    WasiPreview1, ERRNO_BADF, ERRNO_FAULT, ERRNO_SUCCESS, FILETYPE_DIRECTORY, FILETYPE_REGULAR_FILE,
};

const DIRENT_SIZE: usize = 24;
const ERRNO_NOTSUP: i32 = 58;

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

fn module() -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();

    // (i32 fd, i32 buf, i32 buf_len, i64 cookie, i32 bufused) -> i32 errno
    section(
        &mut module,
        1,
        &[1, 0x60, 5, 0x7f, 0x7f, 0x7f, 0x7e, 0x7f, 1, 0x7f],
    );

    let mut imports = vec![2];
    name(&mut imports, "wasi_snapshot_preview1");
    name(&mut imports, "fd_readdir");
    imports.extend([0, 0]);
    name(&mut imports, "env");
    name(&mut imports, "memory");
    imports.extend([2, 0, 1]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[1, 0]);

    let mut exports = vec![1];
    name(&mut exports, "readdir");
    exports.extend([0, 1]);
    section(&mut module, 7, &exports);

    let body = [
        0, 0x20, 0, 0x20, 1, 0x20, 2, 0x20, 3, 0x20, 4, 0x10, 0, 0x0b,
    ];
    let mut code = vec![1];
    u32leb(&mut code, body.len() as u32);
    code.extend(body);
    section(&mut module, 10, &code);
    module
}

fn instantiate(
    memory: &MemoryHandle,
    wasi: &WasiPreview1,
) -> Result<Instance, wasm_runtime::RuntimeError> {
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .unwrap();
    wasi.register(&mut hosts).unwrap();
    Instance::with_hosts(parse_module(&module()).unwrap(), hosts)
}

fn errno(vm: &mut Instance, args: &[Value]) -> i32 {
    let values = vm.invoke_export_values("readdir", args).unwrap();
    let [Value::I32(errno)] = values.as_slice() else {
        panic!("WASI wrapper returned unexpected values: {values:?}");
    };
    *errno
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

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

#[derive(Debug, PartialEq, Eq)]
struct Dirent {
    next: u64,
    ino: u64,
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
            next: read_u64(header, 0),
            ino: read_u64(header, 8),
            filetype: header[20],
            name: bytes[name_start..name_end].to_vec(),
        });
        cursor = name_end;
    }
    entries
}

#[test]
fn preopen_readdir_is_deterministic_and_cookie_addressable() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let wasi = WasiPreview1::new()
        .with_preopen("/sandbox")
        .unwrap()
        .with_read_only_file("/sandbox", "zeta.txt", b"z")
        .unwrap()
        .with_read_only_file("/sandbox", "alpha.txt", b"a")
        .unwrap()
        .with_read_only_file("/sandbox", "middle.txt", b"m")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi).unwrap();

    assert_eq!(
        errno(&mut vm, &readdir_args(3, 64, 512, 0, 32)),
        ERRNO_SUCCESS
    );
    let used = u32::from_le_bytes(memory.read(32, 4).unwrap().try_into().unwrap()) as usize;
    let entries = parse_complete_dirents(&memory.read(64, used).unwrap());
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.name.as_slice())
            .collect::<Vec<_>>(),
        vec![
            b".".as_slice(),
            b"..".as_slice(),
            b"alpha.txt".as_slice(),
            b"middle.txt".as_slice(),
            b"zeta.txt".as_slice()
        ]
    );
    assert_eq!(
        entries.iter().map(|entry| entry.next).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );
    assert_eq!(entries[0].ino, entries[1].ino);
    assert_eq!(
        entries[2..]
            .iter()
            .map(|entry| entry.ino)
            .collect::<Vec<_>>(),
        vec![2, 3, 1]
    );
    assert!(entries[..2]
        .iter()
        .all(|entry| entry.filetype == FILETYPE_DIRECTORY));
    assert!(entries[2..]
        .iter()
        .all(|entry| entry.filetype == FILETYPE_REGULAR_FILE));

    assert_eq!(
        errno(&mut vm, &readdir_args(3, 1024, 512, 3, 36)),
        ERRNO_SUCCESS
    );
    let resumed_used = u32::from_le_bytes(memory.read(36, 4).unwrap().try_into().unwrap()) as usize;
    let resumed = parse_complete_dirents(&memory.read(1024, resumed_used).unwrap());
    assert_eq!(
        resumed
            .iter()
            .map(|entry| entry.name.as_slice())
            .collect::<Vec<_>>(),
        vec![b"middle.txt".as_slice(), b"zeta.txt".as_slice()]
    );
    assert_eq!(
        resumed.iter().map(|entry| entry.next).collect::<Vec<_>>(),
        vec![4, 5]
    );
}

#[test]
fn readdir_reports_exact_truncated_buffer_usage_without_overwrite() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, &[0xaa; 128]).unwrap();
    let wasi = WasiPreview1::new()
        .with_preopen("/sandbox")
        .unwrap()
        .with_read_only_file("/sandbox", "alpha.txt", b"a")
        .unwrap()
        .with_read_only_file("/sandbox", "beta.txt", b"b")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi).unwrap();

    let buf_len = (DIRENT_SIZE + 1 + DIRENT_SIZE + 2 + DIRENT_SIZE + 5) as u32;
    assert_eq!(
        errno(&mut vm, &readdir_args(3, 64, buf_len, 0, 32)),
        ERRNO_SUCCESS
    );
    assert_eq!(
        u32::from_le_bytes(memory.read(32, 4).unwrap().try_into().unwrap()),
        buf_len
    );
    let bytes = memory.read(64, 128).unwrap();
    let complete = parse_complete_dirents(&bytes[..buf_len as usize]);
    assert_eq!(complete.len(), 2);
    assert_eq!(complete[0].name, b".");
    assert_eq!(complete[1].name, b"..");
    let partial_name_start = DIRENT_SIZE + 1 + DIRENT_SIZE + 2 + DIRENT_SIZE;
    assert_eq!(&bytes[partial_name_start..buf_len as usize], b"alpha");
    assert_eq!(&bytes[buf_len as usize..], &[0xaa; 128][buf_len as usize..]);
}

#[test]
fn readdir_failures_are_atomic_and_nested_namespace_is_explicitly_unsupported() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, &[0x5a; 64]).unwrap();
    memory.write(32, &0xfeedfaceu32.to_le_bytes()).unwrap();
    let flat = WasiPreview1::new()
        .with_preopen("/sandbox")
        .unwrap()
        .with_read_only_file("/sandbox", "alpha.txt", b"a")
        .unwrap();
    let mut vm = instantiate(&memory, &flat).unwrap();

    assert_eq!(errno(&mut vm, &readdir_args(99, 64, 32, 0, 32)), ERRNO_BADF);
    assert_eq!(
        u32::from_le_bytes(memory.read(32, 4).unwrap().try_into().unwrap()),
        0xfeedface
    );
    assert_eq!(memory.read(64, 64).unwrap(), vec![0x5a; 64]);

    assert_eq!(
        errno(&mut vm, &readdir_args(3, 65_530, 16, 0, 32)),
        ERRNO_FAULT
    );
    assert_eq!(
        u32::from_le_bytes(memory.read(32, 4).unwrap().try_into().unwrap()),
        0xfeedface
    );
    assert_eq!(memory.read(64, 64).unwrap(), vec![0x5a; 64]);

    let nested_memory = MemoryHandle::new(1, Some(1)).unwrap();
    nested_memory
        .write(32, &0xfeedfaceu32.to_le_bytes())
        .unwrap();
    nested_memory.write(64, &[0x5a; 64]).unwrap();
    let nested = WasiPreview1::new()
        .with_preopen("/sandbox")
        .unwrap()
        .with_read_only_file("/sandbox", "dir/file.txt", b"nested")
        .unwrap();
    let mut nested_vm = instantiate(&nested_memory, &nested).unwrap();
    assert_eq!(
        errno(&mut nested_vm, &readdir_args(3, 64, 32, 0, 32)),
        ERRNO_NOTSUP
    );
    assert_eq!(
        u32::from_le_bytes(nested_memory.read(32, 4).unwrap().try_into().unwrap()),
        0xfeedface
    );
    assert_eq!(nested_memory.read(64, 64).unwrap(), vec![0x5a; 64]);
}
