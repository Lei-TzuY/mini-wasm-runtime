use std::{
    fs,
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
};

use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance as MiniInstance, MemoryHandle, Value};
use wasm_wasi::{WasiPreview1, ERRNO_NOENT, ERRNO_SUCCESS};
use wasmtime::{Engine, Linker, Memory, MemoryType, Module as ReferenceModule, Store};
use wasmtime_wasi::{
    p1::{self, WasiP1Ctx},
    DirPerms, FilePerms, WasiCtxBuilder,
};

const REOPENED_FD: u32 = 40;
const FIRST_FILESTAT: u32 = 64;
const SECOND_FILESTAT: u32 = 128;
const FILESTAT_NLINK_OFFSET: u32 = 24;
const FILESTAT_SIZE_OFFSET: u32 = 32;
const PWRITE_IOV: u32 = 192;
const PWRITE_NWRITTEN: u32 = 204;
const PREAD_IOV: u32 = 208;
const PREAD_NREAD: u32 = 216;
const WRITE_BYTE: u32 = 224;
const READ_BYTES: u32 = 240;
const INITIAL_BYTES: &[u8] = b"abc";
const FINAL_OPEN_BYTES: &[u8] = b"aZc";

#[derive(Debug, Clone, PartialEq, Eq)]
struct UnlinkTrace {
    errnos: [i32; 7],
    nlinks: [u64; 2],
    sizes: [u64; 2],
    reopened_fd_sentinel: u32,
    nwritten: u32,
    nread: u32,
    open_bytes: Vec<u8>,
    pathname_exists: bool,
}

struct IsolatedDirectory {
    path: PathBuf,
}

impl IsolatedDirectory {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mini-wasm-runtime-wasi-unlink-{}-{id}",
            process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).expect("remove stale WASI unlink differential directory");
        }
        fs::create_dir(&path).expect("create isolated WASI unlink differential directory");
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
            (import "wasi_snapshot_preview1" "path_unlink_file"
                (func $path_unlink_file (param i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_filestat_get"
                (func $fd_filestat_get (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_pwrite"
                (func $fd_pwrite (param i32 i32 i32 i64 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_pread"
                (func $fd_pread (param i32 i32 i32 i64 i32) (result i32)))
            (import "env" "memory" (memory 1 1))
            (export "memory" (memory 0))
            (data (i32.const 0) "data.bin")

            (func (export "open") (result i32)
                i32.const 3
                i32.const 0
                i32.const 0
                i32.const 8
                i32.const 0
                i64.const 2097222
                i64.const 0
                i32.const 0
                i32.const 32
                call $path_open)
            (func (export "stat_before") (result i32)
                i32.const 32
                i32.load
                i32.const 64
                call $fd_filestat_get)
            (func (export "unlink") (result i32)
                i32.const 3
                i32.const 0
                i32.const 8
                call $path_unlink_file)
            (func (export "reopen") (result i32)
                i32.const 3
                i32.const 0
                i32.const 0
                i32.const 8
                i32.const 0
                i64.const 2097222
                i64.const 0
                i32.const 0
                i32.const 40
                call $path_open)
            (func (export "stat_after") (result i32)
                i32.const 32
                i32.load
                i32.const 128
                call $fd_filestat_get)
            (func (export "pwrite") (result i32)
                i32.const 32
                i32.load
                i32.const 192
                i32.const 1
                i64.const 1
                i32.const 204
                call $fd_pwrite)
            (func (export "pread") (result i32)
                i32.const 32
                i32.load
                i32.const 208
                i32.const 1
                i64.const 0
                i32.const 216
                call $fd_pread))
        "#,
    )
    .expect("compile deterministic WASI unlink module")
}

fn mini_errno(instance: &mut MiniInstance, export: &str) -> i32 {
    match instance
        .invoke_export(export, &[])
        .unwrap_or_else(|error| panic!("mini WASI unlink call {export:?} trapped: {error:?}"))
    {
        Some(Value::I32(errno)) => errno,
        other => panic!("mini WASI unlink call {export:?} returned {other:?}"),
    }
}

fn mini_u32(memory: &MemoryHandle, address: u32) -> u32 {
    u32::from_le_bytes(
        memory
            .read(address, 4)
            .expect("read mini WASI unlink u32")
            .try_into()
            .expect("fixed u32 width"),
    )
}

fn mini_u64(memory: &MemoryHandle, address: u32) -> u64 {
    u64::from_le_bytes(
        memory
            .read(address, 8)
            .expect("read mini WASI unlink u64")
            .try_into()
            .expect("fixed u64 width"),
    )
}

fn initialize_mini_memory(memory: &MemoryHandle) {
    memory
        .write(REOPENED_FD, &0xfeedface_u32.to_le_bytes())
        .expect("seed mini reopen sentinel");
    memory
        .write(PWRITE_IOV, &WRITE_BYTE.to_le_bytes())
        .expect("seed mini pwrite pointer");
    memory
        .write(PWRITE_IOV + 4, &1_u32.to_le_bytes())
        .expect("seed mini pwrite length");
    memory
        .write(PREAD_IOV, &READ_BYTES.to_le_bytes())
        .expect("seed mini pread pointer");
    memory
        .write(PREAD_IOV + 4, &3_u32.to_le_bytes())
        .expect("seed mini pread length");
    memory
        .write(WRITE_BYTE, b"Z")
        .expect("seed mini pwrite payload");
}

fn run_mini(bytes: &[u8]) -> UnlinkTrace {
    let memory = MemoryHandle::new(1, Some(1)).expect("create mini WASI unlink memory");
    initialize_mini_memory(&memory);
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .expect("configure mini writable preopen")
        .with_writable_file("/sandbox", "data.bin", INITIAL_BYTES)
        .expect("configure mini writable file");
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .expect("register mini WASI unlink memory");
    wasi.register(&mut hosts)
        .expect("register mini WASI unlink surface");
    let module = parse_module(bytes).expect("unlink module must parse in mini runtime");
    let mut instance = MiniInstance::with_hosts(module, hosts)
        .expect("unlink module must instantiate in mini runtime");

    let errnos = [
        mini_errno(&mut instance, "open"),
        mini_errno(&mut instance, "stat_before"),
        mini_errno(&mut instance, "unlink"),
        mini_errno(&mut instance, "reopen"),
        mini_errno(&mut instance, "stat_after"),
        mini_errno(&mut instance, "pwrite"),
        mini_errno(&mut instance, "pread"),
    ];

    UnlinkTrace {
        errnos,
        nlinks: [
            mini_u64(&memory, FIRST_FILESTAT + FILESTAT_NLINK_OFFSET),
            mini_u64(&memory, SECOND_FILESTAT + FILESTAT_NLINK_OFFSET),
        ],
        sizes: [
            mini_u64(&memory, FIRST_FILESTAT + FILESTAT_SIZE_OFFSET),
            mini_u64(&memory, SECOND_FILESTAT + FILESTAT_SIZE_OFFSET),
        ],
        reopened_fd_sentinel: mini_u32(&memory, REOPENED_FD),
        nwritten: mini_u32(&memory, PWRITE_NWRITTEN),
        nread: mini_u32(&memory, PREAD_NREAD),
        open_bytes: memory
            .read(READ_BYTES, FINAL_OPEN_BYTES.len())
            .expect("read mini old-descriptor bytes"),
        pathname_exists: wasi.file_snapshot("/sandbox", "data.bin").is_some(),
    }
}

fn reference_errno(
    instance: &wasmtime::Instance,
    store: &mut Store<WasiP1Ctx>,
    export: &str,
) -> i32 {
    instance
        .get_typed_func::<(), i32>(&mut *store, export)
        .unwrap_or_else(|error| panic!("resolve Wasmtime unlink export {export:?}: {error}"))
        .call(&mut *store, ())
        .unwrap_or_else(|error| panic!("Wasmtime unlink call {export:?} trapped: {error}"))
}

fn reference_u32(memory: Memory, store: &Store<WasiP1Ctx>, address: u32) -> u32 {
    let mut bytes = [0_u8; 4];
    memory
        .read(store, address as usize, &mut bytes)
        .expect("read Wasmtime WASI unlink u32");
    u32::from_le_bytes(bytes)
}

fn reference_u64(memory: Memory, store: &Store<WasiP1Ctx>, address: u32) -> u64 {
    let mut bytes = [0_u8; 8];
    memory
        .read(store, address as usize, &mut bytes)
        .expect("read Wasmtime WASI unlink u64");
    u64::from_le_bytes(bytes)
}

fn initialize_reference_memory(memory: Memory, store: &mut Store<WasiP1Ctx>) {
    memory
        .write(
            &mut *store,
            REOPENED_FD as usize,
            &0xfeedface_u32.to_le_bytes(),
        )
        .expect("seed Wasmtime reopen sentinel");
    memory
        .write(&mut *store, PWRITE_IOV as usize, &WRITE_BYTE.to_le_bytes())
        .expect("seed Wasmtime pwrite pointer");
    memory
        .write(&mut *store, (PWRITE_IOV + 4) as usize, &1_u32.to_le_bytes())
        .expect("seed Wasmtime pwrite length");
    memory
        .write(&mut *store, PREAD_IOV as usize, &READ_BYTES.to_le_bytes())
        .expect("seed Wasmtime pread pointer");
    memory
        .write(&mut *store, (PREAD_IOV + 4) as usize, &3_u32.to_le_bytes())
        .expect("seed Wasmtime pread length");
    memory
        .write(&mut *store, WRITE_BYTE as usize, b"Z")
        .expect("seed Wasmtime pwrite payload");
}

fn run_reference(engine: &Engine, bytes: &[u8]) -> UnlinkTrace {
    let root = IsolatedDirectory::new();
    let host_file = root.path().join("data.bin");
    fs::write(&host_file, INITIAL_BYTES).expect("seed Wasmtime WASI unlink file");

    let module = ReferenceModule::new(engine, bytes).expect("compile WASI unlink module");
    let mut builder = WasiCtxBuilder::new();
    builder
        .preopened_dir(root.path(), "/sandbox", DirPerms::all(), FilePerms::all())
        .expect("configure isolated Wasmtime unlink preopen");
    let mut store = Store::new(engine, builder.build_p1());
    let memory = Memory::new(&mut store, MemoryType::new(1, Some(1)))
        .expect("create Wasmtime WASI unlink memory");
    initialize_reference_memory(memory, &mut store);
    let mut linker: Linker<WasiP1Ctx> = Linker::new(engine);
    p1::add_to_linker_sync(&mut linker, |context| context)
        .expect("register Wasmtime WASI Preview1 unlink imports");
    linker
        .define(&store, "env", "memory", memory)
        .expect("register Wasmtime unlink memory");
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiate unlink module in Wasmtime");

    let errnos = [
        reference_errno(&instance, &mut store, "open"),
        reference_errno(&instance, &mut store, "stat_before"),
        reference_errno(&instance, &mut store, "unlink"),
        reference_errno(&instance, &mut store, "reopen"),
        reference_errno(&instance, &mut store, "stat_after"),
        reference_errno(&instance, &mut store, "pwrite"),
        reference_errno(&instance, &mut store, "pread"),
    ];
    let mut open_bytes = vec![0_u8; FINAL_OPEN_BYTES.len()];
    memory
        .read(&store, READ_BYTES as usize, &mut open_bytes)
        .expect("read Wasmtime old-descriptor bytes");

    UnlinkTrace {
        errnos,
        nlinks: [
            reference_u64(memory, &store, FIRST_FILESTAT + FILESTAT_NLINK_OFFSET),
            reference_u64(memory, &store, SECOND_FILESTAT + FILESTAT_NLINK_OFFSET),
        ],
        sizes: [
            reference_u64(memory, &store, FIRST_FILESTAT + FILESTAT_SIZE_OFFSET),
            reference_u64(memory, &store, SECOND_FILESTAT + FILESTAT_SIZE_OFFSET),
        ],
        reopened_fd_sentinel: reference_u32(memory, &store, REOPENED_FD),
        nwritten: reference_u32(memory, &store, PWRITE_NWRITTEN),
        nread: reference_u32(memory, &store, PREAD_NREAD),
        open_bytes,
        pathname_exists: host_file.exists(),
    }
}

fn expected_trace() -> UnlinkTrace {
    UnlinkTrace {
        errnos: [
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_NOENT,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
        ],
        nlinks: [1, 0],
        sizes: [3, 3],
        reopened_fd_sentinel: 0xfeedface,
        nwritten: 1,
        nread: 3,
        open_bytes: FINAL_OPEN_BYTES.to_vec(),
        pathname_exists: false,
    }
}

#[test]
fn deterministic_unlink_open_descriptor_lifecycle_matches_wasmtime_wasi() {
    let bytes = module_bytes();
    let mini = run_mini(&bytes);
    let reference = run_reference(&Engine::default(), &bytes);
    let expected = expected_trace();

    assert_eq!(mini, expected, "mini WASI unlink lifecycle trace mismatch");
    assert_eq!(
        reference, expected,
        "Wasmtime WASI unlink lifecycle trace mismatch"
    );
    assert_eq!(
        mini, reference,
        "WASI unlink lifecycle differential mismatch"
    );
}
