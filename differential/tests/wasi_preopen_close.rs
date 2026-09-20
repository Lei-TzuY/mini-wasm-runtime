use std::{
    fs,
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
};

use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance as MiniInstance, MemoryHandle, Value};
use wasm_wasi::{
    WasiPreview1, ERRNO_BADF, ERRNO_SUCCESS, FILETYPE_REGULAR_FILE, RIGHTS_FD_READ,
};
use wasmtime::{Engine, Linker, Memory, MemoryType, Module as ReferenceModule, Store};
use wasmtime_wasi::{
    p1::{self, WasiP1Ctx},
    DirPerms, FilePerms, WasiCtxBuilder,
};

const FIRST_PRESTAT: usize = 0;
const FIRST_NAME: usize = 16;
const SECOND_PRESTAT: usize = 32;
const SECOND_NAME: usize = 48;
const CHILD_FD: usize = 96;
const FAILED_FD: usize = 100;
const CLOSED_PRESTAT: usize = 112;
const CLOSED_NAME: usize = 128;
const PATH_PTR: usize = 160;
const CHILD_FDSTAT: usize = 192;
const SNAPSHOT_BYTES: usize = 224;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Trace {
    errnos: [i32; 12],
    first_prestat: Vec<u8>,
    first_name: Vec<u8>,
    second_prestat: Vec<u8>,
    second_name: Vec<u8>,
    closed_prestat: Vec<u8>,
    closed_name: Vec<u8>,
    failed_fd: u32,
    child_filetype: u8,
}

struct IsolatedDirectory {
    path: PathBuf,
}

impl IsolatedDirectory {
    fn new(label: &str) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mini-wasm-runtime-wasi-preopen-close-{label}-{}-{id}",
            process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).expect("remove stale preopen-close differential directory");
        }
        fs::create_dir(&path).expect("create isolated preopen-close differential directory");
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

fn module_bytes() -> Vec<u8> {
    wat::parse_str(format!(
        r#"
        (module
            (import "wasi_snapshot_preview1" "fd_prestat_get"
                (func $fd_prestat_get (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_prestat_dir_name"
                (func $fd_prestat_dir_name (param i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_open"
                (func $path_open
                    (param i32 i32 i32 i32 i32 i64 i64 i32 i32)
                    (result i32)))
            (import "wasi_snapshot_preview1" "fd_close"
                (func $fd_close (param i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_fdstat_get"
                (func $fd_fdstat_get (param i32 i32) (result i32)))
            (import "env" "memory" (memory 1 1))
            (export "memory" (memory 0))
            (data (i32.const {PATH_PTR}) "data.bin")

            (func (export "first_prestat") (result i32)
                i32.const 3
                i32.const {FIRST_PRESTAT}
                call $fd_prestat_get)
            (func (export "first_name") (result i32)
                i32.const 3
                i32.const {FIRST_NAME}
                i32.const 16
                call $fd_prestat_dir_name)
            (func (export "second_prestat") (result i32)
                i32.const 4
                i32.const {SECOND_PRESTAT}
                call $fd_prestat_get)
            (func (export "second_name") (result i32)
                i32.const 4
                i32.const {SECOND_NAME}
                i32.const 16
                call $fd_prestat_dir_name)
            (func (export "open_child") (result i32)
                i32.const 4
                i32.const 0
                i32.const {PATH_PTR}
                i32.const 8
                i32.const 0
                i64.const {RIGHTS_FD_READ}
                i64.const 0
                i32.const 0
                i32.const {CHILD_FD}
                call $path_open)
            (func (export "close_first") (result i32)
                i32.const 3
                call $fd_close)
            (func (export "closed_prestat") (result i32)
                i32.const 3
                i32.const {CLOSED_PRESTAT}
                call $fd_prestat_get)
            (func (export "closed_name") (result i32)
                i32.const 3
                i32.const {CLOSED_NAME}
                i32.const 16
                call $fd_prestat_dir_name)
            (func (export "closed_path_open") (result i32)
                i32.const 3
                i32.const 0
                i32.const {PATH_PTR}
                i32.const 8
                i32.const 0
                i64.const {RIGHTS_FD_READ}
                i64.const 0
                i32.const 0
                i32.const {FAILED_FD}
                call $path_open)
            (func (export "child_stat") (result i32)
                i32.const {CHILD_FD}
                i32.load
                i32.const {CHILD_FDSTAT}
                call $fd_fdstat_get)
            (func (export "close_first_again") (result i32)
                i32.const 3
                call $fd_close)
            (func (export "close_child") (result i32)
                i32.const {CHILD_FD}
                i32.load
                call $fd_close))
        "#,
        RIGHTS_FD_READ = RIGHTS_FD_READ,
    ))
    .expect("compile deterministic WASI preopen-close interop module")
}

fn seed_bytes() -> Vec<u8> {
    let mut bytes = vec![0_u8; SNAPSHOT_BYTES];
    bytes[FIRST_NAME..FIRST_NAME + 16].fill(0xa1);
    bytes[SECOND_NAME..SECOND_NAME + 16].fill(0xa2);
    bytes[CLOSED_PRESTAT..CLOSED_PRESTAT + 8].fill(0xcc);
    bytes[CLOSED_NAME..CLOSED_NAME + 16].fill(0xdd);
    bytes[FAILED_FD..FAILED_FD + 4].copy_from_slice(&0xdead_beefu32.to_le_bytes());
    bytes[CHILD_FDSTAT..CHILD_FDSTAT + 24].fill(0xee);
    bytes
}

fn mini_errno(instance: &mut MiniInstance, export: &str) -> i32 {
    match instance
        .invoke_export(export, &[])
        .unwrap_or_else(|error| panic!("mini preopen-close call {export:?} trapped: {error:?}"))
    {
        Some(Value::I32(errno)) => errno,
        other => panic!("mini preopen-close call {export:?} returned {other:?}"),
    }
}

fn run_mini(bytes: &[u8]) -> Trace {
    let memory = MemoryHandle::new(1, Some(1)).expect("create mini preopen-close memory");
    memory
        .write(0, &seed_bytes())
        .expect("seed mini preopen-close memory");
    let wasi = WasiPreview1::new()
        .with_preopen("/first")
        .unwrap()
        .with_preopen("/second")
        .unwrap()
        .with_read_only_file("/second", "data.bin", b"payload")
        .unwrap();

    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .expect("register mini preopen-close memory");
    wasi.register(&mut hosts)
        .expect("register mini preopen-close WASI surface");
    let module = parse_module(bytes).expect("preopen-close module must parse in mini runtime");
    let mut instance = MiniInstance::with_hosts(module, hosts)
        .expect("preopen-close module must instantiate in mini runtime");

    let errnos = [
        mini_errno(&mut instance, "first_prestat"),
        mini_errno(&mut instance, "first_name"),
        mini_errno(&mut instance, "second_prestat"),
        mini_errno(&mut instance, "second_name"),
        mini_errno(&mut instance, "open_child"),
        mini_errno(&mut instance, "close_first"),
        mini_errno(&mut instance, "closed_prestat"),
        mini_errno(&mut instance, "closed_name"),
        mini_errno(&mut instance, "closed_path_open"),
        mini_errno(&mut instance, "child_stat"),
        mini_errno(&mut instance, "close_first_again"),
        mini_errno(&mut instance, "close_child"),
    ];

    let snapshot = memory
        .read(0, SNAPSHOT_BYTES)
        .expect("read mini preopen-close memory");
    trace_from_snapshot(errnos, &snapshot)
}

fn reference_errno(
    instance: &wasmtime::Instance,
    store: &mut Store<WasiP1Ctx>,
    export: &str,
) -> i32 {
    instance
        .get_typed_func::<(), i32>(&mut *store, export)
        .unwrap_or_else(|error| panic!("resolve Wasmtime preopen-close export {export:?}: {error}"))
        .call(&mut *store, ())
        .unwrap_or_else(|error| panic!("Wasmtime preopen-close call {export:?} trapped: {error}"))
}

fn run_reference(engine: &Engine, bytes: &[u8]) -> Trace {
    let first = IsolatedDirectory::new("first");
    let second = IsolatedDirectory::new("second");
    fs::write(second.path().join("data.bin"), b"payload")
        .expect("seed Wasmtime preopen-close file");

    let module = ReferenceModule::new(engine, bytes).expect("compile preopen-close Wasmtime module");
    let mut builder = WasiCtxBuilder::new();
    builder
        .preopened_dir(first.path(), "/first", DirPerms::all(), FilePerms::all())
        .expect("configure first Wasmtime preopen")
        .preopened_dir(second.path(), "/second", DirPerms::all(), FilePerms::all())
        .expect("configure second Wasmtime preopen");
    let mut store = Store::new(engine, builder.build_p1());
    let memory = Memory::new(&mut store, MemoryType::new(1, Some(1)))
        .expect("create Wasmtime preopen-close memory");
    memory
        .write(&mut store, 0, &seed_bytes())
        .expect("seed Wasmtime preopen-close memory");

    let mut linker: Linker<WasiP1Ctx> = Linker::new(engine);
    p1::add_to_linker_sync(&mut linker, |context| context)
        .expect("register Wasmtime preopen-close imports");
    linker
        .define(&store, "env", "memory", memory)
        .expect("register Wasmtime preopen-close memory");
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiate preopen-close module in Wasmtime");

    let errnos = [
        reference_errno(&instance, &mut store, "first_prestat"),
        reference_errno(&instance, &mut store, "first_name"),
        reference_errno(&instance, &mut store, "second_prestat"),
        reference_errno(&instance, &mut store, "second_name"),
        reference_errno(&instance, &mut store, "open_child"),
        reference_errno(&instance, &mut store, "close_first"),
        reference_errno(&instance, &mut store, "closed_prestat"),
        reference_errno(&instance, &mut store, "closed_name"),
        reference_errno(&instance, &mut store, "closed_path_open"),
        reference_errno(&instance, &mut store, "child_stat"),
        reference_errno(&instance, &mut store, "close_first_again"),
        reference_errno(&instance, &mut store, "close_child"),
    ];

    let mut snapshot = vec![0_u8; SNAPSHOT_BYTES];
    memory
        .read(&store, 0, &mut snapshot)
        .expect("read Wasmtime preopen-close memory");
    trace_from_snapshot(errnos, &snapshot)
}

fn trace_from_snapshot(errnos: [i32; 12], snapshot: &[u8]) -> Trace {
    Trace {
        errnos,
        first_prestat: snapshot[FIRST_PRESTAT..FIRST_PRESTAT + 8].to_vec(),
        first_name: snapshot[FIRST_NAME..FIRST_NAME + 16].to_vec(),
        second_prestat: snapshot[SECOND_PRESTAT..SECOND_PRESTAT + 8].to_vec(),
        second_name: snapshot[SECOND_NAME..SECOND_NAME + 16].to_vec(),
        closed_prestat: snapshot[CLOSED_PRESTAT..CLOSED_PRESTAT + 8].to_vec(),
        closed_name: snapshot[CLOSED_NAME..CLOSED_NAME + 16].to_vec(),
        failed_fd: u32::from_le_bytes(
            snapshot[FAILED_FD..FAILED_FD + 4]
                .try_into()
                .expect("failed fd width"),
        ),
        child_filetype: snapshot[CHILD_FDSTAT],
    }
}

fn expected_trace() -> Trace {
    let mut first_prestat = vec![0_u8; 8];
    first_prestat[4..8].copy_from_slice(&6u32.to_le_bytes());
    let mut second_prestat = vec![0_u8; 8];
    second_prestat[4..8].copy_from_slice(&7u32.to_le_bytes());

    let mut first_name = vec![0xa1; 16];
    first_name[..6].copy_from_slice(b"/first");
    let mut second_name = vec![0xa2; 16];
    second_name[..7].copy_from_slice(b"/second");

    Trace {
        errnos: [
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_BADF,
            ERRNO_BADF,
            ERRNO_BADF,
            ERRNO_SUCCESS,
            ERRNO_BADF,
            ERRNO_SUCCESS,
        ],
        first_prestat,
        first_name,
        second_prestat,
        second_name,
        closed_prestat: vec![0xcc; 8],
        closed_name: vec![0xdd; 16],
        failed_fd: 0xdead_beef,
        child_filetype: FILETYPE_REGULAR_FILE,
    }
}

#[test]
fn preopen_close_liveness_matches_wasmtime_wasi() {
    let bytes = module_bytes();
    let mini = run_mini(&bytes);
    let reference = run_reference(&Engine::default(), &bytes);
    let expected = expected_trace();

    assert_eq!(mini, expected, "mini WASI preopen-close trace mismatch");
    assert_eq!(
        reference, expected,
        "Wasmtime WASI preopen-close trace mismatch"
    );
    assert_eq!(mini, reference, "WASI preopen-close differential mismatch");
}
