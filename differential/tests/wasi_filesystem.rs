use std::{
    fs,
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
};

use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance as MiniInstance, MemoryHandle, Value};
use wasm_wasi::{WasiPreview1, ERRNO_SUCCESS};
use wasmtime::{Engine, Linker, Memory, MemoryType, Module as ReferenceModule, Store};
use wasmtime_wasi::{
    p1::{self, WasiP1Ctx},
    DirPerms, FilePerms, WasiCtxBuilder,
};

const TELL_OFFSET: u32 = 48;
const FIRST_FILESTAT: u32 = 64;
const SECOND_FILESTAT: u32 = 128;
const FILESTAT_SIZE_OFFSET: u32 = 32;
const INITIAL_BYTES: &[u8] = b"abcdef";
const FINAL_BYTES: &[u8] = b"ab\0\0\0";

#[derive(Debug, Clone, PartialEq, Eq)]
struct FilesystemTrace {
    errnos: [i32; 7],
    sizes: [u64; 2],
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
            "mini-wasm-runtime-wasi-filesystem-{}-{id}",
            process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).expect("remove stale WASI filesystem differential directory");
        }
        fs::create_dir(&path).expect("create isolated WASI filesystem differential directory");
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
            (import "wasi_snapshot_preview1" "fd_filestat_set_size"
                (func $fd_filestat_set_size (param i32 i64) (result i32)))
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
                i64.const 6291556
                i64.const 0
                i32.const 0
                i32.const 32
                call $path_open)
            (func (export "seek") (result i32)
                i32.const 32
                i32.load
                i64.const 4
                i32.const 0
                i32.const 40
                call $fd_seek)
            (func (export "shrink") (result i32)
                i32.const 32
                i32.load
                i64.const 2
                call $fd_filestat_set_size)
            (func (export "stat_after_shrink") (result i32)
                i32.const 32
                i32.load
                i32.const 64
                call $fd_filestat_get)
            (func (export "tell_after_shrink") (result i32)
                i32.const 32
                i32.load
                i32.const 48
                call $fd_tell)
            (func (export "extend") (result i32)
                i32.const 32
                i32.load
                i64.const 5
                call $fd_filestat_set_size)
            (func (export "stat_after_extend") (result i32)
                i32.const 32
                i32.load
                i32.const 128
                call $fd_filestat_get))
        "#,
    )
    .expect("compile deterministic WASI filesystem module")
}

fn mini_errno(instance: &mut MiniInstance, export: &str) -> i32 {
    match instance
        .invoke_export(export, &[])
        .unwrap_or_else(|error| panic!("mini WASI filesystem call {export:?} trapped: {error:?}"))
    {
        Some(Value::I32(errno)) => errno,
        other => panic!("mini WASI filesystem call {export:?} returned {other:?}"),
    }
}

fn mini_u64(memory: &MemoryHandle, address: u32) -> u64 {
    u64::from_le_bytes(
        memory
            .read(address, 8)
            .expect("read mini WASI filesystem u64")
            .try_into()
            .expect("fixed u64 width"),
    )
}

fn run_mini(bytes: &[u8]) -> FilesystemTrace {
    let memory = MemoryHandle::new(1, Some(1)).expect("create mini WASI filesystem memory");
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .expect("configure mini writable preopen")
        .with_writable_file("/sandbox", "data.bin", INITIAL_BYTES)
        .expect("configure mini writable file");
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .expect("register mini WASI filesystem memory");
    wasi.register(&mut hosts)
        .expect("register mini WASI filesystem surface");
    let module = parse_module(bytes).expect("filesystem module must parse in mini runtime");
    let mut instance = MiniInstance::with_hosts(module, hosts)
        .expect("filesystem module must instantiate in mini runtime");

    let errnos = [
        mini_errno(&mut instance, "open"),
        mini_errno(&mut instance, "seek"),
        mini_errno(&mut instance, "shrink"),
        mini_errno(&mut instance, "stat_after_shrink"),
        mini_errno(&mut instance, "tell_after_shrink"),
        mini_errno(&mut instance, "extend"),
        mini_errno(&mut instance, "stat_after_extend"),
    ];
    FilesystemTrace {
        errnos,
        sizes: [
            mini_u64(&memory, FIRST_FILESTAT + FILESTAT_SIZE_OFFSET),
            mini_u64(&memory, SECOND_FILESTAT + FILESTAT_SIZE_OFFSET),
        ],
        cursor: mini_u64(&memory, TELL_OFFSET),
        final_bytes: wasi
            .file_snapshot("/sandbox", "data.bin")
            .expect("snapshot mini WASI differential file"),
    }
}

fn reference_errno(
    instance: &wasmtime::Instance,
    store: &mut Store<WasiP1Ctx>,
    export: &str,
) -> i32 {
    instance
        .get_typed_func::<(), i32>(&mut *store, export)
        .unwrap_or_else(|error| panic!("resolve Wasmtime filesystem export {export:?}: {error}"))
        .call(&mut *store, ())
        .unwrap_or_else(|error| panic!("Wasmtime filesystem call {export:?} trapped: {error}"))
}

fn reference_u64(memory: Memory, store: &Store<WasiP1Ctx>, address: u32) -> u64 {
    let mut bytes = [0_u8; 8];
    memory
        .read(store, address as usize, &mut bytes)
        .expect("read Wasmtime WASI filesystem u64");
    u64::from_le_bytes(bytes)
}

fn run_reference(engine: &Engine, bytes: &[u8]) -> FilesystemTrace {
    let root = IsolatedDirectory::new();
    let host_file = root.path().join("data.bin");
    fs::write(&host_file, INITIAL_BYTES).expect("seed Wasmtime WASI differential file");

    let module = ReferenceModule::new(engine, bytes).expect("compile WASI filesystem module");
    let mut builder = WasiCtxBuilder::new();
    builder
        .preopened_dir(root.path(), "/sandbox", DirPerms::all(), FilePerms::all())
        .expect("configure isolated Wasmtime writable preopen");
    let mut store = Store::new(engine, builder.build_p1());
    let memory = Memory::new(&mut store, MemoryType::new(1, Some(1)))
        .expect("create Wasmtime WASI filesystem memory");
    let mut linker: Linker<WasiP1Ctx> = Linker::new(engine);
    p1::add_to_linker_sync(&mut linker, |context| context)
        .expect("register Wasmtime WASI Preview1 filesystem imports");
    linker
        .define(&store, "env", "memory", memory)
        .expect("register Wasmtime filesystem memory");
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiate filesystem module in Wasmtime");

    let errnos = [
        reference_errno(&instance, &mut store, "open"),
        reference_errno(&instance, &mut store, "seek"),
        reference_errno(&instance, &mut store, "shrink"),
        reference_errno(&instance, &mut store, "stat_after_shrink"),
        reference_errno(&instance, &mut store, "tell_after_shrink"),
        reference_errno(&instance, &mut store, "extend"),
        reference_errno(&instance, &mut store, "stat_after_extend"),
    ];
    let sizes = [
        reference_u64(memory, &store, FIRST_FILESTAT + FILESTAT_SIZE_OFFSET),
        reference_u64(memory, &store, SECOND_FILESTAT + FILESTAT_SIZE_OFFSET),
    ];
    let cursor = reference_u64(memory, &store, TELL_OFFSET);
    drop(store);
    let final_bytes = fs::read(host_file).expect("read Wasmtime WASI differential file");

    FilesystemTrace {
        errnos,
        sizes,
        cursor,
        final_bytes,
    }
}

fn expected_trace() -> FilesystemTrace {
    FilesystemTrace {
        errnos: [ERRNO_SUCCESS; 7],
        sizes: [2, 5],
        cursor: 4,
        final_bytes: FINAL_BYTES.to_vec(),
    }
}

#[test]
fn deterministic_regular_file_resize_trace_matches_wasmtime_wasi() {
    let bytes = module_bytes();
    let mini = run_mini(&bytes);
    let reference = run_reference(&Engine::default(), &bytes);
    let expected = expected_trace();

    assert_eq!(mini, expected, "mini WASI filesystem resize trace mismatch");
    assert_eq!(
        reference, expected,
        "Wasmtime WASI filesystem resize trace mismatch"
    );
    assert_eq!(
        mini, reference,
        "WASI filesystem resize differential mismatch"
    );
}
