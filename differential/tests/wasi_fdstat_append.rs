use std::{
    fs,
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
};

use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance as MiniInstance, MemoryHandle, Value};
use wasm_wasi::{
    WasiPreview1, ERRNO_SUCCESS, FDFLAGS_APPEND, RIGHTS_FD_FDSTAT_SET_FLAGS, RIGHTS_FD_SEEK,
    RIGHTS_FD_WRITE,
};
use wasmtime::{Engine, Linker, Memory, MemoryType, Module as ReferenceModule, Store};
use wasmtime_wasi::{
    p1::{self, WasiP1Ctx},
    DirPerms, FilePerms, WasiCtxBuilder,
};

const FDSTAT_APPEND_PTR: u32 = 96;
const FDSTAT_CLEAR_PTR: u32 = 128;
const INITIAL_BYTES: &[u8] = b"abc";
const FINAL_BYTES: &[u8] = b"YbcZ";

#[derive(Debug, Clone, PartialEq, Eq)]
struct AppendTrace {
    errnos: [i32; 9],
    append_flags: u16,
    clear_flags: u16,
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
            "mini-wasm-runtime-wasi-append-{}-{id}",
            process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).expect("remove stale APPEND differential directory");
        }
        fs::create_dir(&path).expect("create isolated APPEND differential directory");
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
    let rights = RIGHTS_FD_WRITE | RIGHTS_FD_SEEK | RIGHTS_FD_FDSTAT_SET_FLAGS;
    wat::parse_str(format!(
        r#"
        (module
            (import "wasi_snapshot_preview1" "path_open"
                (func $path_open
                    (param i32 i32 i32 i32 i32 i64 i64 i32 i32)
                    (result i32)))
            (import "wasi_snapshot_preview1" "fd_fdstat_set_flags"
                (func $fd_fdstat_set_flags (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_fdstat_get"
                (func $fd_fdstat_get (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_seek"
                (func $fd_seek (param i32 i64 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_write"
                (func $fd_write (param i32 i32 i32 i32) (result i32)))
            (import "env" "memory" (memory 1 1))
            (export "memory" (memory 0))
            (data (i32.const 0) "data.bin")
            (data (i32.const 64) "\50\00\00\00\01\00\00\00\51\00\00\00\01\00\00\00")
            (data (i32.const 80) "ZY")

            (func (export "open") (result i32)
                i32.const 3
                i32.const 0
                i32.const 0
                i32.const 8
                i32.const 0
                i64.const {rights}
                i64.const 0
                i32.const 0
                i32.const 32
                call $path_open)
            (func (export "set_append") (result i32)
                i32.const 32
                i32.load
                i32.const {append_flag}
                call $fd_fdstat_set_flags)
            (func (export "stat_append") (result i32)
                i32.const 32
                i32.load
                i32.const 96
                call $fd_fdstat_get)
            (func (export "seek_zero") (result i32)
                i32.const 32
                i32.load
                i64.const 0
                i32.const 0
                i32.const 40
                call $fd_seek)
            (func (export "write_z") (result i32)
                i32.const 32
                i32.load
                i32.const 64
                i32.const 1
                i32.const 48
                call $fd_write)
            (func (export "clear_append") (result i32)
                i32.const 32
                i32.load
                i32.const 0
                call $fd_fdstat_set_flags)
            (func (export "stat_clear") (result i32)
                i32.const 32
                i32.load
                i32.const 128
                call $fd_fdstat_get)
            (func (export "write_y") (result i32)
                i32.const 32
                i32.load
                i32.const 72
                i32.const 1
                i32.const 52
                call $fd_write))
        "#,
        rights = rights,
        append_flag = FDFLAGS_APPEND
    ))
    .expect("compile deterministic WASI APPEND module")
}

fn mini_errno(instance: &mut MiniInstance, export: &str) -> i32 {
    match instance
        .invoke_export(export, &[])
        .unwrap_or_else(|error| panic!("mini APPEND call {export:?} trapped: {error:?}"))
    {
        Some(Value::I32(errno)) => errno,
        other => panic!("mini APPEND call {export:?} returned {other:?}"),
    }
}

fn mini_u16(memory: &MemoryHandle, address: u32) -> u16 {
    u16::from_le_bytes(
        memory
            .read(address, 2)
            .expect("read mini APPEND u16")
            .try_into()
            .expect("fixed u16 width"),
    )
}

fn run_mini(bytes: &[u8]) -> AppendTrace {
    let memory = MemoryHandle::new(1, Some(1)).expect("create mini APPEND memory");
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .expect("configure mini APPEND preopen")
        .with_writable_file("/sandbox", "data.bin", INITIAL_BYTES)
        .expect("configure mini APPEND file");
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .expect("register mini APPEND memory");
    wasi.register(&mut hosts)
        .expect("register mini APPEND WASI surface");
    let module = parse_module(bytes).expect("APPEND module must parse in mini runtime");
    let mut instance =
        MiniInstance::with_hosts(module, hosts).expect("APPEND module must instantiate");

    let errnos = [
        mini_errno(&mut instance, "open"),
        mini_errno(&mut instance, "set_append"),
        mini_errno(&mut instance, "stat_append"),
        mini_errno(&mut instance, "seek_zero"),
        mini_errno(&mut instance, "write_z"),
        mini_errno(&mut instance, "clear_append"),
        mini_errno(&mut instance, "stat_clear"),
        mini_errno(&mut instance, "seek_zero"),
        mini_errno(&mut instance, "write_y"),
    ];

    AppendTrace {
        errnos,
        append_flags: mini_u16(&memory, FDSTAT_APPEND_PTR + 2),
        clear_flags: mini_u16(&memory, FDSTAT_CLEAR_PTR + 2),
        final_bytes: wasi
            .file_snapshot("/sandbox", "data.bin")
            .expect("snapshot mini APPEND file"),
    }
}

fn reference_errno(
    instance: &wasmtime::Instance,
    store: &mut Store<WasiP1Ctx>,
    export: &str,
) -> i32 {
    instance
        .get_typed_func::<(), i32>(&mut *store, export)
        .unwrap_or_else(|error| panic!("resolve Wasmtime APPEND export {export:?}: {error}"))
        .call(&mut *store, ())
        .unwrap_or_else(|error| panic!("Wasmtime APPEND call {export:?} trapped: {error}"))
}

fn reference_u16(memory: Memory, store: &Store<WasiP1Ctx>, address: u32) -> u16 {
    let mut bytes = [0_u8; 2];
    memory
        .read(store, address as usize, &mut bytes)
        .expect("read Wasmtime APPEND u16");
    u16::from_le_bytes(bytes)
}

fn run_reference(engine: &Engine, bytes: &[u8]) -> AppendTrace {
    let root = IsolatedDirectory::new();
    let host_file = root.path().join("data.bin");
    fs::write(&host_file, INITIAL_BYTES).expect("seed Wasmtime APPEND file");

    let module = ReferenceModule::new(engine, bytes).expect("compile APPEND module in Wasmtime");
    let mut builder = WasiCtxBuilder::new();
    builder
        .preopened_dir(root.path(), "/sandbox", DirPerms::all(), FilePerms::all())
        .expect("configure isolated Wasmtime APPEND preopen");
    let mut store = Store::new(engine, builder.build_p1());
    let memory = Memory::new(&mut store, MemoryType::new(1, Some(1)))
        .expect("create Wasmtime APPEND memory");
    let mut linker: Linker<WasiP1Ctx> = Linker::new(engine);
    p1::add_to_linker_sync(&mut linker, |context| context)
        .expect("register Wasmtime APPEND imports");
    linker
        .define(&store, "env", "memory", memory)
        .expect("register Wasmtime APPEND memory");
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiate APPEND module in Wasmtime");

    let errnos = [
        reference_errno(&instance, &mut store, "open"),
        reference_errno(&instance, &mut store, "set_append"),
        reference_errno(&instance, &mut store, "stat_append"),
        reference_errno(&instance, &mut store, "seek_zero"),
        reference_errno(&instance, &mut store, "write_z"),
        reference_errno(&instance, &mut store, "clear_append"),
        reference_errno(&instance, &mut store, "stat_clear"),
        reference_errno(&instance, &mut store, "seek_zero"),
        reference_errno(&instance, &mut store, "write_y"),
    ];
    let append_flags = reference_u16(memory, &store, FDSTAT_APPEND_PTR + 2);
    let clear_flags = reference_u16(memory, &store, FDSTAT_CLEAR_PTR + 2);
    drop(store);
    let final_bytes = fs::read(host_file).expect("read Wasmtime APPEND file");

    AppendTrace {
        errnos,
        append_flags,
        clear_flags,
        final_bytes,
    }
}

fn expected_trace() -> AppendTrace {
    AppendTrace {
        errnos: [ERRNO_SUCCESS; 9],
        append_flags: FDFLAGS_APPEND,
        clear_flags: 0,
        final_bytes: FINAL_BYTES.to_vec(),
    }
}

#[test]
fn descriptor_append_lifecycle_matches_wasmtime_wasi() {
    let bytes = module_bytes();
    let mini = run_mini(&bytes);
    let reference = run_reference(&Engine::default(), &bytes);
    let expected = expected_trace();

    assert_eq!(mini, expected, "mini WASI APPEND trace mismatch");
    assert_eq!(reference, expected, "Wasmtime WASI APPEND trace mismatch");
    assert_eq!(mini, reference, "WASI APPEND differential mismatch");
}
