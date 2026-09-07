use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, Value};
use wasm_wasi::{
    WasiPreopenError, WasiPreview1, ERRNO_BADF, ERRNO_FAULT, ERRNO_NAMETOOLONG, ERRNO_SUCCESS,
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

fn i32_const(out: &mut Vec<u8>, value: u32) {
    out.push(0x41);
    let mut value = value as i32;
    loop {
        let mut byte = (value as u8) & 0x7f;
        let sign_bit_set = byte & 0x40 != 0;
        value >>= 7;
        let done = (value == 0 && !sign_bit_set) || (value == -1 && sign_bit_set);
        if !done {
            byte |= 0x80;
        }
        out.push(byte);
        if done {
            break;
        }
    }
}

fn module(import_name: &str, args: &[u32]) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();

    let mut types = vec![2, 0x60, args.len() as u8];
    types.extend(std::iter::repeat_n(0x7f, args.len()));
    types.extend([1, 0x7f, 0x60, 0, 1, 0x7f]);
    section(&mut module, 1, &types);

    let mut imports = vec![2];
    name(&mut imports, "wasi_snapshot_preview1");
    name(&mut imports, import_name);
    imports.extend([0, 0]);
    name(&mut imports, "env");
    name(&mut imports, "memory");
    imports.extend([2, 0, 1]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[1, 1]);

    let mut exports = vec![1];
    name(&mut exports, "run");
    exports.extend([0, 1]);
    section(&mut module, 7, &exports);

    let mut body = vec![0];
    for arg in args {
        i32_const(&mut body, *arg);
    }
    body.extend([0x10, 0, 0x0b]);
    let mut code = vec![1];
    u32leb(&mut code, body.len() as u32);
    code.extend(body);
    section(&mut module, 10, &code);
    module
}

fn instantiate(
    import_name: &str,
    args: &[u32],
    memory: &MemoryHandle,
    wasi: &WasiPreview1,
) -> Instance {
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .unwrap();
    wasi.register(&mut hosts).unwrap();
    Instance::with_hosts(parse_module(&module(import_name, args)).unwrap(), hosts).unwrap()
}

#[test]
fn fd_prestat_get_reports_directory_tag_and_guest_name_length() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(32, &[0xaa; 8]).unwrap();
    let wasi = WasiPreview1::new().with_preopen("/sandbox").unwrap();
    let mut vm = instantiate("fd_prestat_get", &[3, 32], &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    let prestat = memory.read(32, 8).unwrap();
    assert_eq!(prestat[0], 0);
    assert_eq!(&prestat[1..4], &[0, 0, 0]);
    assert_eq!(u32::from_le_bytes(prestat[4..8].try_into().unwrap()), 8);
}

#[test]
fn preopens_allocate_after_stdio_in_configuration_order() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let wasi = WasiPreview1::new()
        .with_preopen("/first")
        .unwrap()
        .with_preopen("/second")
        .unwrap();
    let mut vm = instantiate("fd_prestat_get", &[4, 32], &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    let prestat = memory.read(32, 8).unwrap();
    assert_eq!(u32::from_le_bytes(prestat[4..8].try_into().unwrap()), 7);
}

#[test]
fn fd_prestat_dir_name_writes_name_without_nul_and_preserves_extra_capacity() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, &[0xaa; 16]).unwrap();
    let wasi = WasiPreview1::new().with_preopen("/sandbox").unwrap();
    let mut vm = instantiate("fd_prestat_dir_name", &[3, 64, 16], &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    assert_eq!(memory.read(64, 8).unwrap(), b"/sandbox");
    assert_eq!(memory.read(72, 8).unwrap(), vec![0xaa; 8]);
}

#[test]
fn short_preopen_name_buffer_fails_without_guest_mutation() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(64, &[0xbb; 8]).unwrap();
    let wasi = WasiPreview1::new().with_preopen("/sandbox").unwrap();
    let mut vm = instantiate("fd_prestat_dir_name", &[3, 64, 7], &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_NAMETOOLONG))
    );
    assert_eq!(memory.read(64, 8).unwrap(), vec![0xbb; 8]);
}

#[test]
fn unknown_preopen_fd_is_rejected_before_guest_memory_access() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let wasi = WasiPreview1::new().with_preopen("/sandbox").unwrap();
    let mut get = instantiate("fd_prestat_get", &[4, 65_532], &memory, &wasi);
    assert_eq!(
        get.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_BADF))
    );

    let mut name = instantiate("fd_prestat_dir_name", &[4, 65_532, 8], &memory, &wasi);
    assert_eq!(
        name.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_BADF))
    );
}

#[test]
fn valid_preopen_oob_destinations_fail_closed() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let wasi = WasiPreview1::new().with_preopen("/sandbox").unwrap();
    let mut get = instantiate("fd_prestat_get", &[3, 65_532], &memory, &wasi);
    assert_eq!(
        get.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_FAULT))
    );

    let mut name = instantiate("fd_prestat_dir_name", &[3, 65_532, 8], &memory, &wasi);
    assert_eq!(
        name.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_FAULT))
    );
}

#[test]
fn preopen_configuration_is_bounded_before_registration() {
    assert!(matches!(
        WasiPreview1::new().with_preopen(""),
        Err(WasiPreopenError::EmptyGuestPath)
    ));

    let too_long = "x".repeat(4 * 1024 + 1);
    assert!(matches!(
        WasiPreview1::new().with_preopen(&too_long),
        Err(WasiPreopenError::NameTooLong { .. })
    ));

    let mut wasi = WasiPreview1::new();
    for index in 0..128 {
        wasi = wasi.with_preopen(format!("/preopen-{index}")).unwrap();
    }
    assert!(matches!(
        wasi.with_preopen("/overflow"),
        Err(WasiPreopenError::TooManyPreopens { limit: 128 })
    ));
}
