use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, Value};
use wasm_wasi::{
    WasiClockId, WasiPreview1, ERRNO_FAULT, ERRNO_INVAL, ERRNO_NOTSUP, ERRNO_SUCCESS,
};

const SUBSCRIPTION_SIZE: usize = 48;
const EVENT_SIZE: usize = 32;
const EVENTTYPE_CLOCK: u8 = 0;
const SUBCLOCKFLAGS_ABSTIME: u16 = 1;

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

fn name(out: &mut Vec<u8>, value: &str) {
    u32leb(out, value.len() as u32);
    out.extend_from_slice(value.as_bytes());
}

fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    u32leb(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn poll_module(input: u32, output: u32, count: u32, nevents: u32) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(
        &mut module,
        1,
        &[
            2,
            0x60,
            4,
            0x7f,
            0x7f,
            0x7f,
            0x7f,
            1,
            0x7f,
            0x60,
            0,
            1,
            0x7f,
        ],
    );

    let mut imports = vec![2];
    name(&mut imports, "wasi_snapshot_preview1");
    name(&mut imports, "poll_oneoff");
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
    for value in [input, output, count, nevents] {
        body.push(0x41);
        i32leb(&mut body, value as i32);
    }
    body.extend([0x10, 0, 0x0b]);
    let mut code = vec![1];
    u32leb(&mut code, body.len() as u32);
    code.extend(body);
    section(&mut module, 10, &code);
    module
}

fn subscription(userdata: u64, clock_id: u32, timeout: u64, precision: u64, flags: u16) -> [u8; SUBSCRIPTION_SIZE] {
    let mut bytes = [0u8; SUBSCRIPTION_SIZE];
    bytes[0..8].copy_from_slice(&userdata.to_le_bytes());
    bytes[8] = EVENTTYPE_CLOCK;
    bytes[16..20].copy_from_slice(&clock_id.to_le_bytes());
    bytes[24..32].copy_from_slice(&timeout.to_le_bytes());
    bytes[32..40].copy_from_slice(&precision.to_le_bytes());
    bytes[40..42].copy_from_slice(&flags.to_le_bytes());
    bytes
}

fn instantiate(bytes: &[u8], memory: &MemoryHandle, wasi: &WasiPreview1) -> Instance {
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .unwrap();
    wasi.register(&mut hosts).unwrap();
    Instance::with_hosts(parse_module(bytes).unwrap(), hosts).unwrap()
}

fn errno(vm: &mut Instance) -> i32 {
    match vm.invoke_export("run", &[]).unwrap() {
        Some(Value::I32(value)) => value,
        other => panic!("unexpected poll_oneoff result: {other:?}"),
    }
}

fn event_userdata(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes[0..8].try_into().unwrap())
}

#[test]
fn ready_clock_subscriptions_emit_exact_preview1_events() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let input = 64u32;
    let output = 256u32;
    let nevents = 400u32;
    let first = subscription(0x1122_3344_5566_7788, 0, 0, 7, 0);
    let second = subscription(0x8877_6655_4433_2211, 1, 400, 9, SUBCLOCKFLAGS_ABSTIME);
    memory.write(input, &first).unwrap();
    memory
        .write(input + SUBSCRIPTION_SIZE as u32, &second)
        .unwrap();
    memory.write(output, &[0xaa; EVENT_SIZE * 2]).unwrap();
    memory.write(nevents, &0xdead_beefu32.to_le_bytes()).unwrap();

    let wasi = WasiPreview1::new()
        .with_clock(WasiClockId::Realtime, 1, 1_000)
        .with_clock(WasiClockId::Monotonic, 1, 500);
    let mut vm = instantiate(&poll_module(input, output, 2, nevents), &memory, &wasi);

    assert_eq!(errno(&mut vm), ERRNO_SUCCESS);
    assert_eq!(
        u32::from_le_bytes(memory.read(nevents, 4).unwrap().try_into().unwrap()),
        2
    );

    for (index, expected_userdata) in [
        0x1122_3344_5566_7788u64,
        0x8877_6655_4433_2211u64,
    ]
    .into_iter()
    .enumerate()
    {
        let event = memory
            .read(output + (index * EVENT_SIZE) as u32, EVENT_SIZE)
            .unwrap();
        assert_eq!(event_userdata(&event), expected_userdata);
        assert_eq!(&event[8..10], &0u16.to_le_bytes());
        assert_eq!(event[10], EVENTTYPE_CLOCK);
        assert_eq!(&event[11..], &[0; EVENT_SIZE - 11]);
    }
}

#[test]
fn future_timer_fails_closed_without_partial_output() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let input = 64u32;
    let output = 256u32;
    let nevents = 400u32;
    memory
        .write(input, &subscription(7, 1, 1, 0, 0))
        .unwrap();
    memory.write(output, &[0xaa; EVENT_SIZE]).unwrap();
    memory.write(nevents, &0xdead_beefu32.to_le_bytes()).unwrap();

    let wasi = WasiPreview1::new().with_clock(WasiClockId::Monotonic, 1, 500);
    let mut vm = instantiate(&poll_module(input, output, 1, nevents), &memory, &wasi);

    assert_eq!(errno(&mut vm), ERRNO_NOTSUP);
    assert_eq!(memory.read(output, EVENT_SIZE).unwrap(), vec![0xaa; EVENT_SIZE]);
    assert_eq!(
        u32::from_le_bytes(memory.read(nevents, 4).unwrap().try_into().unwrap()),
        0xdead_beef
    );
}

#[test]
fn invalid_clock_or_flags_fail_closed() {
    for subscription in [
        subscription(1, 9, 0, 0, 0),
        subscription(1, 0, 0, 0, 2),
    ] {
        let memory = MemoryHandle::new(1, Some(1)).unwrap();
        memory.write(64, &subscription).unwrap();
        memory.write(256, &[0xbb; EVENT_SIZE]).unwrap();
        memory.write(400, &0xfeed_faceu32.to_le_bytes()).unwrap();

        let wasi = WasiPreview1::new().with_clock(WasiClockId::Realtime, 1, 100);
        let mut vm = instantiate(&poll_module(64, 256, 1, 400), &memory, &wasi);
        assert_eq!(errno(&mut vm), ERRNO_INVAL);
        assert_eq!(memory.read(256, EVENT_SIZE).unwrap(), vec![0xbb; EVENT_SIZE]);
        assert_eq!(
            u32::from_le_bytes(memory.read(400, 4).unwrap().try_into().unwrap()),
            0xfeed_face
        );
    }
}

#[test]
fn zero_subscriptions_and_oob_buffers_are_rejected_without_writes() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(400, &0x1234_5678u32.to_le_bytes()).unwrap();
    let wasi = WasiPreview1::new().with_clock(WasiClockId::Realtime, 1, 100);

    let mut empty = instantiate(&poll_module(64, 256, 0, 400), &memory, &wasi);
    assert_eq!(errno(&mut empty), ERRNO_INVAL);
    assert_eq!(
        u32::from_le_bytes(memory.read(400, 4).unwrap().try_into().unwrap()),
        0x1234_5678
    );

    let mut oob = instantiate(&poll_module(65_520, 256, 1, 400), &memory, &wasi);
    assert_eq!(errno(&mut oob), ERRNO_FAULT);
    assert_eq!(
        u32::from_le_bytes(memory.read(400, 4).unwrap().try_into().unwrap()),
        0x1234_5678
    );
}
