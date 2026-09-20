use std::time::Duration;

use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance as MiniInstance, MemoryHandle, Value};
use wasm_wasi::{WasiClockId, WasiPreview1, ERRNO_SUCCESS};
use wasmtime::{Engine, Linker, Memory, MemoryType, Module as ReferenceModule, Store};
use wasmtime_wasi::{
    p1::{self, WasiP1Ctx},
    Deterministic, HostMonotonicClock, HostWallClock, WasiCtxBuilder,
};

const SNAPSHOT_BYTES: usize = 64;
const REALTIME_RES_PTR: usize = 0;
const REALTIME_TIME_PTR: usize = 8;
const MONOTONIC_RES_PTR: usize = 16;
const MONOTONIC_TIME_PTR: usize = 24;
const RANDOM_FIRST_PTR: usize = 32;
const RANDOM_SECOND_PTR: usize = 40;

const ENTROPY: &[u8] = &[0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0xa1, 0xb2, 0xc3, 0xd4];
const REALTIME_RESOLUTION_NS: u64 = 1_000_000;
const REALTIME_TIME_NS: u64 = 1_700_000_000_123_456_789;
const MONOTONIC_RESOLUTION_NS: u64 = 100;
const MONOTONIC_TIME_NS: u64 = 9_876_543_210;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Trace {
    errnos: [i32; 6],
    memory: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
struct FixedWallClock {
    resolution: Duration,
    now: Duration,
}

impl HostWallClock for FixedWallClock {
    fn resolution(&self) -> Duration {
        self.resolution
    }

    fn now(&self) -> Duration {
        self.now
    }
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

fn module_bytes() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (import "wasi_snapshot_preview1" "random_get"
                (func $random_get (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "clock_res_get"
                (func $clock_res_get (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "clock_time_get"
                (func $clock_time_get (param i32 i64 i32) (result i32)))
            (import "env" "memory" (memory 1 1))
            (export "memory" (memory 0))

            (func (export "realtime_res") (result i32)
                i32.const 0
                i32.const 0
                call $clock_res_get)
            (func (export "realtime_time") (result i32)
                i32.const 0
                i64.const 0
                i32.const 8
                call $clock_time_get)
            (func (export "monotonic_res") (result i32)
                i32.const 1
                i32.const 16
                call $clock_res_get)
            (func (export "monotonic_time") (result i32)
                i32.const 1
                i64.const 0
                i32.const 24
                call $clock_time_get)
            (func (export "random_first") (result i32)
                i32.const 32
                i32.const 6
                call $random_get)
            (func (export "random_second") (result i32)
                i32.const 40
                i32.const 4
                call $random_get))
        "#,
    )
    .expect("compile deterministic WASI clock/entropy interop module")
}

fn mini_errno(instance: &mut MiniInstance, export: &str) -> i32 {
    match instance.invoke_export(export, &[]).unwrap_or_else(|error| {
        panic!("mini WASI clock/entropy call {export:?} trapped: {error:?}")
    }) {
        Some(Value::I32(errno)) => errno,
        other => panic!("mini WASI clock/entropy call {export:?} returned {other:?}"),
    }
}

fn run_mini(bytes: &[u8]) -> Trace {
    let memory = MemoryHandle::new(1, Some(1)).expect("create mini clock/entropy memory");
    let wasi = WasiPreview1::new()
        .with_random_bytes(ENTROPY)
        .with_clock(
            WasiClockId::Realtime,
            REALTIME_RESOLUTION_NS,
            REALTIME_TIME_NS,
        )
        .with_clock(
            WasiClockId::Monotonic,
            MONOTONIC_RESOLUTION_NS,
            MONOTONIC_TIME_NS,
        );

    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .expect("register mini clock/entropy memory");
    wasi.register(&mut hosts)
        .expect("register mini WASI clock/entropy surface");

    let module = parse_module(bytes).expect("clock/entropy module must parse in mini runtime");
    let mut instance = MiniInstance::with_hosts(module, hosts)
        .expect("clock/entropy module must instantiate in mini runtime");

    let errnos = [
        mini_errno(&mut instance, "realtime_res"),
        mini_errno(&mut instance, "realtime_time"),
        mini_errno(&mut instance, "monotonic_res"),
        mini_errno(&mut instance, "monotonic_time"),
        mini_errno(&mut instance, "random_first"),
        mini_errno(&mut instance, "random_second"),
    ];

    Trace {
        errnos,
        memory: memory
            .read(0, SNAPSHOT_BYTES)
            .expect("read mini clock/entropy memory"),
    }
}

fn reference_errno(
    instance: &wasmtime::Instance,
    store: &mut Store<WasiP1Ctx>,
    export: &str,
) -> i32 {
    instance
        .get_typed_func::<(), i32>(&mut *store, export)
        .unwrap_or_else(|error| panic!("resolve Wasmtime clock/entropy export {export:?}: {error}"))
        .call(&mut *store, ())
        .unwrap_or_else(|error| panic!("Wasmtime clock/entropy call {export:?} trapped: {error}"))
}

fn run_reference(engine: &Engine, bytes: &[u8]) -> Trace {
    let module =
        ReferenceModule::new(engine, bytes).expect("compile clock/entropy module in Wasmtime");

    let mut builder = WasiCtxBuilder::new();
    builder
        .secure_random(Deterministic::new(ENTROPY.to_vec()))
        .wall_clock(FixedWallClock {
            resolution: Duration::from_nanos(REALTIME_RESOLUTION_NS),
            now: Duration::from_nanos(REALTIME_TIME_NS),
        })
        .monotonic_clock(FixedMonotonicClock {
            resolution: MONOTONIC_RESOLUTION_NS,
            now: MONOTONIC_TIME_NS,
        });

    let mut store = Store::new(engine, builder.build_p1());
    let memory = Memory::new(&mut store, MemoryType::new(1, Some(1)))
        .expect("create Wasmtime clock/entropy memory");
    let mut linker: Linker<WasiP1Ctx> = Linker::new(engine);
    p1::add_to_linker_sync(&mut linker, |context| context)
        .expect("register Wasmtime WASI Preview1 clock/entropy imports");
    linker
        .define(&store, "env", "memory", memory)
        .expect("register Wasmtime clock/entropy memory");
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiate clock/entropy module in Wasmtime");

    let errnos = [
        reference_errno(&instance, &mut store, "realtime_res"),
        reference_errno(&instance, &mut store, "realtime_time"),
        reference_errno(&instance, &mut store, "monotonic_res"),
        reference_errno(&instance, &mut store, "monotonic_time"),
        reference_errno(&instance, &mut store, "random_first"),
        reference_errno(&instance, &mut store, "random_second"),
    ];

    let mut snapshot = vec![0_u8; SNAPSHOT_BYTES];
    memory
        .read(&store, 0, &mut snapshot)
        .expect("read Wasmtime clock/entropy memory");

    Trace {
        errnos,
        memory: snapshot,
    }
}

fn write_u64(memory: &mut [u8], address: usize, value: u64) {
    memory[address..address + 8].copy_from_slice(&value.to_le_bytes());
}

fn expected_trace() -> Trace {
    let mut memory = vec![0_u8; SNAPSHOT_BYTES];
    write_u64(&mut memory, REALTIME_RES_PTR, REALTIME_RESOLUTION_NS);
    write_u64(&mut memory, REALTIME_TIME_PTR, REALTIME_TIME_NS);
    write_u64(&mut memory, MONOTONIC_RES_PTR, MONOTONIC_RESOLUTION_NS);
    write_u64(&mut memory, MONOTONIC_TIME_PTR, MONOTONIC_TIME_NS);
    memory[RANDOM_FIRST_PTR..RANDOM_FIRST_PTR + 6].copy_from_slice(&ENTROPY[..6]);
    memory[RANDOM_SECOND_PTR..RANDOM_SECOND_PTR + 4].copy_from_slice(&ENTROPY[6..]);

    Trace {
        errnos: [ERRNO_SUCCESS; 6],
        memory,
    }
}

#[test]
fn deterministic_clocks_and_entropy_match_wasmtime_wasi() {
    let bytes = module_bytes();
    let mini = run_mini(&bytes);
    let reference = run_reference(&Engine::default(), &bytes);
    let expected = expected_trace();

    assert_eq!(mini, expected, "mini WASI clock/entropy trace mismatch");
    assert_eq!(
        reference, expected,
        "Wasmtime WASI clock/entropy trace mismatch"
    );
    assert_eq!(mini, reference, "WASI clock/entropy differential mismatch");
}
