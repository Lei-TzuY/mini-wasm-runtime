use std::{
    fs,
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
};

use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance as MiniInstance, MemoryHandle, Value};
use wasm_wasi::{WasiPreview1, ERRNO_NOTSUP, ERRNO_SUCCESS};
use wasmtime::{Engine, Linker, Memory, MemoryType, Module as ReferenceModule, Store};
use wasmtime_wasi::{
    p1::{self, WasiP1Ctx},
    DirPerms, FilePerms, WasiCtxBuilder,
};

const FILESTAT_PTR: u32 = 64;
const FILESTAT_SIZE_OFFSET: u32 = 32;
const TELL_PTR: u32 = 128;
const INITIAL_BYTES: &[u8] = b"abc";
const MINI_FINAL_BYTES: &[u8] = b"abc\0\0\0\0\0";

#[derive(Debug, Clone, PartialEq, Eq)]
struct AllocationTrace {
    errnos: [i32; 5],
    size: u64,
    cursor: u64,
    final_bytes: Vec<u8>,
}

struct IsolatedDirectory {
    path: PathBuf,
}

impl IsolatedDirectory {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mini-wasm-runtime-wasi-fd-allocate-{}-{id}",
            process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).expect("remove stale fd_allocate differential directory");
        }
        fs::create_dir(&path).expect("create isolated fd_allocate differential directory");
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
    wat::parse_str(
        r#"
        (module
            (import "wasi_snapshot_preview1" "path_open"
                (func $path_open
                    (param i32 i32 i32 i32 i32 i64 i64 i32 i32)
                    (result i32)))
            (import "wasi_snapshot_preview1" "fd_seek"
                (func $fd_seek (param i32 i64 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_allocate"
                (func $fd_allocate (param i32 i64 i64) (result i32)))
            (import "wasi_snapshot_preview1" "fd_filestat_get"
                (func $fd_filestat_get (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_tell"
                (func $fd_tell (param i32 i32) (result i32)))
            (import "env" "memory" (memory 1 1))
            (export "memory" (memory 0))
            (data (i32.const 0) "data.bin")

            (func (export "open") (result i32)
                i32.const 3
                i32.const 0
                i32.const 0
                i32.const 8
                i32.const 0
                i64.const 2097508
                i64.const 0
                i32.const 0
                i32.const 32
                call $path_open)
            (func (export "seek") (result i32)
                i32.const 32
                i32.load
                i64.const 2
                i32.const 0
                i32.const 40
                call $fd_seek)
            (func (export "allocate") (result i32)
                i32.const 32
                i32.load
                i64.const 5
                i64.const 3
                call $fd_allocate)
            (func (export "stat") (result i32)
                i32.const 32
                i32.load
                i32.const 64
                call $fd_filestat_get)
            (func (export "tell") (result i32)
                i32.const 32
                i32.load
                i32.const 128
                call $fd_tell))
        "#,
    )
    .expect("compile deterministic fd_allocate module")
}

fn mini_errno(instance: &mut MiniInstance, export: &str) -> i32 {
    match instance
        .invoke_export(export, &[])
        .unwrap_or_else(|error| panic!("mini fd_allocate call {export:?} trapped: {error:?}"))
    {
        Some(Value::I32(errno)) => errno,
        other => panic!("mini fd_allocate call {export:?} returned {other:?}"),
    }
}

fn mini_u64(memory: &MemoryHandle, address: u32) -> u64 {
    u64::from_le_bytes(
        memory
            .read(address, 8)
            .expect("read mini fd_allocate u64")
            .try_into()
            .expect("fixed u64 width"),
    )
}

fn run_mini(bytes: &[u8]) -> AllocationTrace {
    let memory = MemoryHandle::new(1, Some(1)).expect("create mini fd_allocate memory");
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .expect("configure mini writable preopen")
        .with_writable_file("/sandbox", "data.bin", INITIAL_BYTES)
        .expect("configure mini writable file");
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .expect("register mini fd_allocate memory");
    wasi.register(&mut hosts)
        .expect("register mini WASI fd_allocate surface");
    let module = parse_module(bytes).expect("fd_allocate module must parse in mini runtime");
    let mut instance = MiniInstance::with_hosts(module, hosts)
        .expect("fd_allocate module must instantiate in mini runtime");

    let errnos = [
        mini_errno(&mut instance, "open"),
        mini_errno(&mut instance, "seek"),
        mini_errno(&mut instance, "allocate"),
        mini_errno(&mut instance, "stat"),
        mini_errno(&mut instance, "tell"),
    ];
    AllocationTrace {
        errnos,
        size: mini_u64(&memory, FILESTAT_PTR + FILESTAT_SIZE_OFFSET),
        cursor: mini_u64(&memory, TELL_PTR),
        final_bytes: wasi
            .file_snapshot("/sandbox", "data.bin")
            .expect("snapshot mini fd_allocate file"),
    }
}

fn reference_errno(
    instance: &wasmtime::Instance,
    store: &mut Store<WasiP1Ctx>,
    export: &str,
) -> i32 {
    instance
        .get_typed_func::<(), i32>(&mut *store, export)
        .unwrap_or_else(|error| panic!("resolve Wasmtime fd_allocate export {export:?}: {error}"))
        .call(&mut *store, ())
        .unwrap_or_else(|error| panic!("Wasmtime fd_allocate call {export:?} trapped: {error}"))
}

fn reference_u64(memory: Memory, store: &Store<WasiP1Ctx>, address: u32) -> u64 {
    let mut bytes = [0_u8; 8];
    memory
        .read(store, address as usize, &mut bytes)
        .expect("read Wasmtime fd_allocate u64");
    u64::from_le_bytes(bytes)
}

fn run_reference(engine: &Engine, bytes: &[u8]) -> AllocationTrace {
    let root = IsolatedDirectory::new();
    let host_file = root.path().join("data.bin");
    fs::write(&host_file, INITIAL_BYTES).expect("seed Wasmtime fd_allocate file");

    let module =
        ReferenceModule::new(engine, bytes).expect("compile fd_allocate module in Wasmtime");
    let mut builder = WasiCtxBuilder::new();
    builder
        .preopened_dir(root.path(), "/sandbox", DirPerms::all(), FilePerms::all())
        .expect("configure isolated Wasmtime fd_allocate preopen");
    let mut store = Store::new(engine, builder.build_p1());
    let memory = Memory::new(&mut store, MemoryType::new(1, Some(1)))
        .expect("create Wasmtime fd_allocate memory");
    let mut linker: Linker<WasiP1Ctx> = Linker::new(engine);
    p1::add_to_linker_sync(&mut linker, |context| context)
        .expect("register Wasmtime WASI Preview1 fd_allocate imports");
    linker
        .define(&store, "env", "memory", memory)
        .expect("register Wasmtime fd_allocate memory");
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiate fd_allocate module in Wasmtime");

    let errnos = [
        reference_errno(&instance, &mut store, "open"),
        reference_errno(&instance, &mut store, "seek"),
        reference_errno(&instance, &mut store, "allocate"),
        reference_errno(&instance, &mut store, "stat"),
        reference_errno(&instance, &mut store, "tell"),
    ];
    let size = reference_u64(memory, &store, FILESTAT_PTR + FILESTAT_SIZE_OFFSET);
    let cursor = reference_u64(memory, &store, TELL_PTR);
    drop(store);
    let final_bytes = fs::read(host_file).expect("read Wasmtime fd_allocate file");

    AllocationTrace {
        errnos,
        size,
        cursor,
        final_bytes,
    }
}

#[test]
fn bounded_fd_allocate_capability_is_explicitly_contrasted_with_wasmtime_37() {
    let bytes = module_bytes();
    let mini = run_mini(&bytes);
    let reference = run_reference(&Engine::default(), &bytes);

    assert_eq!(mini.errnos, [ERRNO_SUCCESS; 5]);
    assert_eq!(mini.size, 8);
    assert_eq!(mini.cursor, 2);
    assert_eq!(mini.final_bytes, MINI_FINAL_BYTES);

    assert_eq!(
        reference.errnos[0], ERRNO_SUCCESS,
        "reference path_open failed"
    );
    assert_eq!(
        reference.errnos[1], ERRNO_SUCCESS,
        "reference fd_seek failed"
    );
    assert_eq!(
        reference.errnos[2], ERRNO_NOTSUP,
        "pinned Wasmtime 37 is expected to report fd_allocate as unsupported"
    );
    assert_eq!(
        reference.errnos[3], ERRNO_SUCCESS,
        "reference filestat failed"
    );
    assert_eq!(
        reference.errnos[4], ERRNO_SUCCESS,
        "reference fd_tell failed"
    );
    assert_eq!(reference.size, 3);
    assert_eq!(reference.cursor, 2);
    assert_eq!(reference.final_bytes, INITIAL_BYTES);
}
