use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance as MiniInstance, MemoryHandle, Value};
use wasm_wasi::{WasiClockId, WasiPreview1, ERRNO_SUCCESS};
use wasmtime::{Engine, Linker, Memory, MemoryType, Module as ReferenceModule, Store};
use wasmtime_wasi::{
    p1::{self, WasiP1Ctx},
    HostMonotonicClock, WasiCtxBuilder,
};

const INPUT: usize = 64;
const OUTPUT: usize = 256;
const NEVENTS: usize = 400;
const SUBSCRIPTION_SIZE: usize = 48;
const EVENT_SIZE: usize = 32;
const MONOTONIC_NOW: u64 = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
struct EventTrace {
    userdata: u64,
    error: u16,
    event_type: u8,
    nbytes: u64,
    flags: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Trace {
    errno: i32,
    nevents: u32,
    events: Vec<EventTrace>,
}

#[derive(Debug, Clone, Copy)]
struct FixedMonotonicClock {
    resolution: u64,
    now: u64,
}

impl HostMonotonicClock for FixedMonotonicClock {
    fn resolution(&self) -> u64 {
        self.resolution
    }

    fn now(&self) -> u64 {
        self.now
    }
}

fn fixture_bytes() -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module
            (import "wasi_snapshot_preview1" "poll_oneoff"
                (func $poll_oneoff (param i32 i32 i32 i32) (result i32)))
            (import "env" "memory" (memory 1 1))
            (export "memory" (memory 0))
            (func (export "run") (result i32)
                i32.const {INPUT}
                i32.const {OUTPUT}
                i32.const 2
                i32.const {NEVENTS}
                call $poll_oneoff))"#
    ))
    .expect("compile poll_oneoff differential fixture")
}

fn subscription(userdata: u64, timeout: u64, flags: u16) -> [u8; SUBSCRIPTION_SIZE] {
    let mut bytes = [0u8; SUBSCRIPTION_SIZE];
    bytes[0..8].copy_from_slice(&userdata.to_le_bytes());
    bytes[8] = 0; // eventtype::clock
    bytes[16..20].copy_from_slice(&1u32.to_le_bytes()); // clockid::monotonic
    bytes[24..32].copy_from_slice(&timeout.to_le_bytes());
    bytes[40..42].copy_from_slice(&flags.to_le_bytes());
    bytes
}

fn seed() -> Vec<u8> {
    let mut bytes = vec![0u8; 65_536];
    bytes[INPUT..INPUT + SUBSCRIPTION_SIZE]
        .copy_from_slice(&subscription(0x1122_3344_5566_7788, 0, 0));
    bytes[INPUT + SUBSCRIPTION_SIZE..INPUT + SUBSCRIPTION_SIZE * 2]
        .copy_from_slice(&subscription(0x8877_6655_4433_2211, 400, 1));
    bytes[OUTPUT..OUTPUT + EVENT_SIZE * 2].fill(0xaa);
    bytes[NEVENTS..NEVENTS + 4].copy_from_slice(&0xdead_beefu32.to_le_bytes());
    bytes
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn trace(errno: i32, memory: &[u8]) -> Trace {
    let mut events = Vec::new();
    for index in 0..2 {
        let start = OUTPUT + index * EVENT_SIZE;
        events.push(EventTrace {
            userdata: read_u64(memory, start),
            error: read_u16(memory, start + 8),
            event_type: memory[start + 10],
            nbytes: read_u64(memory, start + 16),
            flags: read_u16(memory, start + 24),
        });
    }
    Trace {
        errno,
        nevents: read_u32(memory, NEVENTS),
        events,
    }
}

fn run_mini(bytes: &[u8]) -> Trace {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(0, &seed()).unwrap();

    let wasi = WasiPreview1::new().with_clock(WasiClockId::Monotonic, 1, MONOTONIC_NOW);
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .unwrap();
    wasi.register(&mut hosts).unwrap();
    let module = parse_module(bytes).unwrap();
    let mut instance = MiniInstance::with_hosts(module, hosts).unwrap();

    let errno = match instance.invoke_export("run", &[]).unwrap() {
        Some(Value::I32(errno)) => errno,
        other => panic!("unexpected mini poll_oneoff result: {other:?}"),
    };
    trace(errno, &memory.read(0, 65_536).unwrap())
}

fn run_reference(engine: &Engine, bytes: &[u8]) -> Trace {
    let module = ReferenceModule::new(engine, bytes).unwrap();
    let mut builder = WasiCtxBuilder::new();
    builder.monotonic_clock(FixedMonotonicClock {
        resolution: 1,
        now: MONOTONIC_NOW,
    });
    let mut store = Store::new(engine, builder.build_p1());
    let memory = Memory::new(&mut store, MemoryType::new(1, Some(1))).unwrap();
    memory.write(&mut store, 0, &seed()).unwrap();

    let mut linker: Linker<WasiP1Ctx> = Linker::new(engine);
    p1::add_to_linker_sync(&mut linker, |context| context).unwrap();
    linker.define(&store, "env", "memory", memory).unwrap();
    let instance = linker.instantiate(&mut store, &module).unwrap();
    let errno = instance
        .get_typed_func::<(), i32>(&mut store, "run")
        .unwrap()
        .call(&mut store, ())
        .unwrap();

    let mut snapshot = vec![0u8; 65_536];
    memory.read(&store, 0, &mut snapshot).unwrap();
    trace(errno, &snapshot)
}

#[test]
fn ready_clock_poll_oneoff_matches_wasmtime_wasi() {
    let bytes = fixture_bytes();
    let mini = run_mini(&bytes);
    let reference = run_reference(&Engine::default(), &bytes);
    let expected = Trace {
        errno: ERRNO_SUCCESS,
        nevents: 2,
        events: vec![
            EventTrace {
                userdata: 0x1122_3344_5566_7788,
                error: 0,
                event_type: 0,
                nbytes: 0,
                flags: 0,
            },
            EventTrace {
                userdata: 0x8877_6655_4433_2211,
                error: 0,
                event_type: 0,
                nbytes: 0,
                flags: 0,
            },
        ],
    };

    assert_eq!(mini, expected, "mini poll_oneoff trace drifted");
    assert_eq!(reference, expected, "Wasmtime poll_oneoff trace drifted");
    assert_eq!(mini, reference, "poll_oneoff differential mismatch");
}
