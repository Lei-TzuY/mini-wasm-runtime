use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, Value};
use wasm_wasi::{WasiClockId, WasiPreview1, ERRNO_FAULT, ERRNO_INVAL, ERRNO_SUCCESS};

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

fn i32leb(out: &mut Vec<u8>, mut value: i32) {
    loop {
        let byte = (value as u8) & 0x7f;
        value >>= 7;
        let done = (value == 0 && byte & 0x40 == 0) || (value == -1 && byte & 0x40 != 0);
        out.push(if done { byte } else { byte | 0x80 });
        if done {
            break;
        }
    }
}

fn i64leb(out: &mut Vec<u8>, mut value: i64) {
    loop {
        let byte = (value as u8) & 0x7f;
        value >>= 7;
        let done = (value == 0 && byte & 0x40 == 0) || (value == -1 && byte & 0x40 != 0);
        out.push(if done { byte } else { byte | 0x80 });
        if done {
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

fn body(code: Vec<u8>) -> Vec<u8> {
    let mut body = vec![0];
    body.extend(code);
    let mut encoded = Vec::new();
    u32leb(&mut encoded, body.len() as u32);
    encoded.extend(body);
    encoded
}

fn clock_res_module(clock_id: u32, result: u32) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(
        &mut module,
        1,
        &[2, 0x60, 2, 0x7f, 0x7f, 1, 0x7f, 0x60, 0, 1, 0x7f],
    );

    let mut imports = vec![2];
    name(&mut imports, "wasi_snapshot_preview1");
    name(&mut imports, "clock_res_get");
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

    let mut run = vec![0x41];
    i32leb(&mut run, clock_id as i32);
    run.push(0x41);
    i32leb(&mut run, result as i32);
    run.extend([0x10, 0, 0x0b]);
    let mut code = vec![1];
    code.extend(body(run));
    section(&mut module, 10, &code);
    module
}

fn clock_time_module(clock_id: u32, precision: i64, result: u32) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(
        &mut module,
        1,
        &[
            2, 0x60, 3, 0x7f, 0x7e, 0x7f, 1, 0x7f, 0x60, 0, 1, 0x7f,
        ],
    );

    let mut imports = vec![2];
    name(&mut imports, "wasi_snapshot_preview1");
    name(&mut imports, "clock_time_get");
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

    let mut run = vec![0x41];
    i32leb(&mut run, clock_id as i32);
    run.push(0x42);
    i64leb(&mut run, precision);
    run.push(0x41);
    i32leb(&mut run, result as i32);
    run.extend([0x10, 0, 0x0b]);
    let mut code = vec![1];
    code.extend(body(run));
    section(&mut module, 10, &code);
    module
}

fn instantiate(bytes: &[u8], memory: &MemoryHandle, wasi: &WasiPreview1) -> Instance {
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .unwrap();
    wasi.register(&mut hosts).unwrap();
    Instance::with_hosts(parse_module(bytes).unwrap(), hosts).unwrap()
}

fn read_u64(memory: &MemoryHandle, address: u32) -> u64 {
    u64::from_le_bytes(memory.read(address, 8).unwrap().try_into().unwrap())
}

#[test]
fn clock_res_get_writes_explicitly_injected_resolution() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let wasi = WasiPreview1::new().with_clock(WasiClockId::Realtime, 1_000_000, 123);
    let mut vm = instantiate(&clock_res_module(0, 32), &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    assert_eq!(read_u64(&memory, 32), 1_000_000);
}

#[test]
fn clock_time_get_returns_deterministic_snapshot_across_calls() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let timestamp = 1_700_000_000_123_456_789u64;
    let wasi = WasiPreview1::new().with_clock(WasiClockId::Monotonic, 100, timestamp);
    let mut vm = instantiate(&clock_time_module(1, 10_000, 32), &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    assert_eq!(read_u64(&memory, 32), timestamp);

    memory.write(32, &[0; 8]).unwrap();
    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    assert_eq!(read_u64(&memory, 32), timestamp);
}

#[test]
fn unconfigured_clock_fails_closed_without_touching_guest_memory() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(32, &[0xaa; 8]).unwrap();
    let wasi = WasiPreview1::new().with_clock(WasiClockId::Realtime, 1, 2);
    let mut vm = instantiate(&clock_time_module(1, 0, 32), &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_INVAL))
    );
    assert_eq!(memory.read(32, 8).unwrap(), vec![0xaa; 8]);
}

#[test]
fn invalid_clock_id_is_rejected_before_guest_memory_access() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(32, &[0xbb; 8]).unwrap();
    let wasi = WasiPreview1::new().with_clock(WasiClockId::Realtime, 1, 2);
    let mut vm = instantiate(&clock_res_module(4, 32), &memory, &wasi);

    assert_eq!(
        vm.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_INVAL))
    );
    assert_eq!(memory.read(32, 8).unwrap(), vec![0xbb; 8]);
}

#[test]
fn clock_time_get_oob_result_is_fault_and_snapshot_remains_available() {
    let wasi = WasiPreview1::new().with_clock(WasiClockId::ThreadCpuTime, 50, 9_999);
    let bad_memory = MemoryHandle::new(1, Some(1)).unwrap();
    let mut bad = instantiate(&clock_time_module(3, 1, 65_532), &bad_memory, &wasi);

    assert_eq!(
        bad.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_FAULT))
    );

    let good_memory = MemoryHandle::new(1, Some(1)).unwrap();
    let mut good = instantiate(&clock_time_module(3, 1, 32), &good_memory, &wasi);
    assert_eq!(
        good.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(ERRNO_SUCCESS))
    );
    assert_eq!(read_u64(&good_memory, 32), 9_999);
}
