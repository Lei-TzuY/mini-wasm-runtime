use std::{
    fs,
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
};

use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance as MiniInstance, MemoryHandle, Value};
use wasm_wasi::{
    WasiPreview1, ERRNO_SUCCESS, RIGHTS_FD_READ, RIGHTS_FD_SEEK, RIGHTS_FD_WRITE,
};
use wasmtime::{Engine, Linker, Memory, MemoryType, Module as ReferenceModule, Store};
use wasmtime_wasi::{
    p1::{self, WasiP1Ctx},
    DirPerms, FilePerms, WasiCtxBuilder,
};

const FD_PTR: usize = 32;
const SEEK_RESULT: usize = 40;
const INPUT: usize = 64;
const OUTPUT: usize = 256;
const NEVENTS: usize = 400;
const SUBSCRIPTION_SIZE: usize = 48;
const EVENT_SIZE: usize = 32;
const EVENTTYPE_FD_READ: u8 = 1;
const EVENTTYPE_FD_WRITE: u8 = 2;
const EVENTRWFLAGS_FD_READWRITE_HANGUP: u16 = 1;
const INITIAL_BYTES: &[u8] = b"abcdef";

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
    errnos: [i32; 4],
    opened_fd: u32,
    pair_nevents: u32,
    pair_events: Vec<EventTrace>,
    seek_position: u64,
    eof_nevents: u32,
    eof_event: EventTrace,
}

struct IsolatedDirectory {
    path: PathBuf,
}

impl IsolatedDirectory {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mini-wasm-runtime-wasi-poll-fd-{}-{id}",
            process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).expect("remove stale poll-fd differential directory");
        }
        fs::create_dir(&path).expect("create isolated poll-fd differential directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for IsolatedDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn fixture_bytes() -> Vec<u8> {
    let rights = RIGHTS_FD_READ | RIGHTS_FD_WRITE | RIGHTS_FD_SEEK;
    wat::parse_str(format!(
        r#"(module
            (import "wasi_snapshot_preview1" "path_open"
                (func $path_open
                    (param i32 i32 i32 i32 i32 i64 i64 i32 i32)
                    (result i32)))
            (import "wasi_snapshot_preview1" "fd_seek"
                (func $fd_seek (param i32 i64 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "poll_oneoff"
                (func $poll_oneoff (param i32 i32 i32 i32) (result i32)))
            (import "env" "memory" (memory 1 1))
            (export "memory" (memory 0))
            (data (i32.const 0) "data.bin")

            (func (export "open") (result i32)
                i32.const 3
                i32.const 0
                i32.const 0
                i32.const 8
                i32.const 0
                i64.const {rights}
                i64.const 0
                i32.const 0
                i32.const {FD_PTR}
                call $path_open)

            (func (export "poll_pair") (result i32)
                i32.const {INPUT}
                i32.const {OUTPUT}
                i32.const 2
                i32.const {NEVENTS}
                call $poll_oneoff)

            (func (export "seek_end") (result i32)
                i32.const {FD_PTR}
                i32.load
                i64.const 0
                i32.const 2
                i32.const {SEEK_RESULT}
                call $fd_seek)

            (func (export "poll_eof") (result i32)
                i32.const {INPUT}
                i32.const {OUTPUT}
                i32.const 1
                i32.const {NEVENTS}
                call $poll_oneoff))"#,
    ))
    .expect("compile poll-fd differential fixture")
}

fn subscription(userdata: u64, event_type: u8, fd: u32) -> [u8; SUBSCRIPTION_SIZE] {
    let mut bytes = [0u8; SUBSCRIPTION_SIZE];
    bytes[0..8].copy_from_slice(&userdata.to_le_bytes());
    bytes[8] = event_type;
    bytes[16..20].copy_from_slice(&fd.to_le_bytes());
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

fn event(memory: &[u8], start: usize) -> EventTrace {
    EventTrace {
        userdata: read_u64(memory, start),
        error: read_u16(memory, start + 8),
        event_type: memory[start + 10],
        nbytes: read_u64(memory, start + 16),
        flags: read_u16(memory, start + 24),
    }
}

fn seed_subscriptions(memory: &MemoryHandle, fd: u32, eof_only: bool) {
    let read = subscription(0x1111_2222_3333_4444, EVENTTYPE_FD_READ, fd);
    memory.write(INPUT as u32, &read).unwrap();
    if !eof_only {
        let write = subscription(0xaaaa_bbbb_cccc_dddd, EVENTTYPE_FD_WRITE, fd);
        memory
            .write((INPUT + SUBSCRIPTION_SIZE) as u32, &write)
            .unwrap();
    }
    memory
        .write(OUTPUT as u32, &[0xaa; EVENT_SIZE * 2])
        .unwrap();
    memory
        .write(NEVENTS as u32, &0xdead_beefu32.to_le_bytes())
        .unwrap();
}

fn mini_errno(instance: &mut MiniInstance, export: &str) -> i32 {
    match instance
        .invoke_export(export, &[])
        .unwrap_or_else(|error| panic!("mini poll-fd call {export:?} trapped: {error:?}"))
    {
        Some(Value::I32(errno)) => errno,
        other => panic!("mini poll-fd call {export:?} returned {other:?}"),
    }
}

fn run_mini(bytes: &[u8]) -> Trace {
    let memory = MemoryHandle::new(1, Some(1)).expect("create mini poll-fd memory");
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap()
        .with_writable_file("/sandbox", "data.bin", INITIAL_BYTES)
        .unwrap();
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .unwrap();
    wasi.register(&mut hosts).unwrap();
    let module = parse_module(bytes).expect("mini parses poll-fd fixture");
    let mut instance = MiniInstance::with_hosts(module, hosts).expect("mini instantiates poll-fd");

    let open_errno = mini_errno(&mut instance, "open");
    let fd = read_u32(&memory.read(FD_PTR as u32, 4).unwrap(), 0);

    seed_subscriptions(&memory, fd, false);
    let pair_errno = mini_errno(&mut instance, "poll_pair");
    let pair_snapshot = memory.read(0, 512).unwrap();
    let pair_nevents = read_u32(&pair_snapshot, NEVENTS);
    let pair_events = vec![
        event(&pair_snapshot, OUTPUT),
        event(&pair_snapshot, OUTPUT + EVENT_SIZE),
    ];

    let seek_errno = mini_errno(&mut instance, "seek_end");
    let seek_position = read_u64(&memory.read(SEEK_RESULT as u32, 8).unwrap(), 0);

    seed_subscriptions(&memory, fd, true);
    let eof_errno = mini_errno(&mut instance, "poll_eof");
    let eof_snapshot = memory.read(0, 512).unwrap();

    Trace {
        errnos: [open_errno, pair_errno, seek_errno, eof_errno],
        opened_fd: fd,
        pair_nevents,
        pair_events,
        seek_position,
        eof_nevents: read_u32(&eof_snapshot, NEVENTS),
        eof_event: event(&eof_snapshot, OUTPUT),
    }
}

fn reference_errno(
    instance: &wasmtime::Instance,
    store: &mut Store<WasiP1Ctx>,
    export: &str,
) -> i32 {
    instance
        .get_typed_func::<(), i32>(&mut *store, export)
        .unwrap_or_else(|error| panic!("resolve Wasmtime poll-fd export {export:?}: {error}"))
        .call(&mut *store, ())
        .unwrap_or_else(|error| panic!("Wasmtime poll-fd call {export:?} trapped: {error}"))
}

fn reference_u32(memory: Memory, store: &Store<WasiP1Ctx>, address: usize) -> u32 {
    let mut bytes = [0u8; 4];
    memory.read(store, address, &mut bytes).unwrap();
    u32::from_le_bytes(bytes)
}

fn reference_u64(memory: Memory, store: &Store<WasiP1Ctx>, address: usize) -> u64 {
    let mut bytes = [0u8; 8];
    memory.read(store, address, &mut bytes).unwrap();
    u64::from_le_bytes(bytes)
}

fn seed_reference(memory: Memory, store: &mut Store<WasiP1Ctx>, fd: u32, eof_only: bool) {
    let read = subscription(0x1111_2222_3333_4444, EVENTTYPE_FD_READ, fd);
    memory.write(&mut *store, INPUT, &read).unwrap();
    if !eof_only {
        let write = subscription(0xaaaa_bbbb_cccc_dddd, EVENTTYPE_FD_WRITE, fd);
        memory
            .write(&mut *store, INPUT + SUBSCRIPTION_SIZE, &write)
            .unwrap();
    }
    memory
        .write(&mut *store, OUTPUT, &[0xaa; EVENT_SIZE * 2])
        .unwrap();
    memory
        .write(&mut *store, NEVENTS, &0xdead_beefu32.to_le_bytes())
        .unwrap();
}

fn reference_snapshot(memory: Memory, store: &Store<WasiP1Ctx>) -> Vec<u8> {
    let mut bytes = vec![0u8; 512];
    memory.read(store, 0, &mut bytes).unwrap();
    bytes
}

fn run_reference(engine: &Engine, bytes: &[u8]) -> Trace {
    let root = IsolatedDirectory::new();
    fs::write(root.path().join("data.bin"), INITIAL_BYTES).unwrap();

    let module = ReferenceModule::new(engine, bytes).expect("compile Wasmtime poll-fd fixture");
    let mut builder = WasiCtxBuilder::new();
    builder
        .preopened_dir(root.path(), "/sandbox", DirPerms::all(), FilePerms::all())
        .unwrap();
    let mut store = Store::new(engine, builder.build_p1());
    let memory = Memory::new(&mut store, MemoryType::new(1, Some(1))).unwrap();
    let mut linker: Linker<WasiP1Ctx> = Linker::new(engine);
    p1::add_to_linker_sync(&mut linker, |context| context).unwrap();
    linker.define(&store, "env", "memory", memory).unwrap();
    let instance = linker.instantiate(&mut store, &module).unwrap();

    let open_errno = reference_errno(&instance, &mut store, "open");
    let fd = reference_u32(memory, &store, FD_PTR);

    seed_reference(memory, &mut store, fd, false);
    let pair_errno = reference_errno(&instance, &mut store, "poll_pair");
    let pair_snapshot = reference_snapshot(memory, &store);
    let pair_nevents = read_u32(&pair_snapshot, NEVENTS);
    let pair_events = vec![
        event(&pair_snapshot, OUTPUT),
        event(&pair_snapshot, OUTPUT + EVENT_SIZE),
    ];

    let seek_errno = reference_errno(&instance, &mut store, "seek_end");
    let seek_position = reference_u64(memory, &store, SEEK_RESULT);

    seed_reference(memory, &mut store, fd, true);
    let eof_errno = reference_errno(&instance, &mut store, "poll_eof");
    let eof_snapshot = reference_snapshot(memory, &store);

    Trace {
        errnos: [open_errno, pair_errno, seek_errno, eof_errno],
        opened_fd: fd,
        pair_nevents,
        pair_events,
        seek_position,
        eof_nevents: read_u32(&eof_snapshot, NEVENTS),
        eof_event: event(&eof_snapshot, OUTPUT),
    }
}

fn expected_trace() -> Trace {
    Trace {
        errnos: [ERRNO_SUCCESS; 4],
        opened_fd: 4,
        pair_nevents: 2,
        pair_events: vec![
            EventTrace {
                userdata: 0x1111_2222_3333_4444,
                error: 0,
                event_type: EVENTTYPE_FD_READ,
                nbytes: 1,
                flags: 0,
            },
            EventTrace {
                userdata: 0xaaaa_bbbb_cccc_dddd,
                error: 0,
                event_type: EVENTTYPE_FD_WRITE,
                nbytes: 1,
                flags: 0,
            },
        ],
        seek_position: INITIAL_BYTES.len() as u64,
        eof_nevents: 1,
        eof_event: EventTrace {
            userdata: 0x1111_2222_3333_4444,
            error: 0,
            event_type: EVENTTYPE_FD_READ,
            nbytes: 1,
            flags: EVENTRWFLAGS_FD_READWRITE_HANGUP,
        },
    }
}

#[test]
fn regular_file_poll_readiness_matches_wasmtime_wasi() {
    let bytes = fixture_bytes();
    let mini = run_mini(&bytes);
    let reference = run_reference(&Engine::default(), &bytes);
    let expected = expected_trace();

    assert_eq!(mini, expected, "mini poll-fd trace drifted");
    assert_eq!(reference, expected, "Wasmtime poll-fd trace drifted");
    assert_eq!(mini, reference, "poll-fd differential mismatch");
}
