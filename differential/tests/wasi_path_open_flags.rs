use std::{
    fs,
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
};

use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance as MiniInstance, MemoryHandle, Value};
use wasm_wasi::{
    WasiPreview1, ERRNO_EXIST, ERRNO_SUCCESS, OFLAGS_CREAT, OFLAGS_EXCL, OFLAGS_TRUNC,
    RIGHTS_FD_READ, RIGHTS_FD_WRITE,
};
use wasmtime::{Engine, Linker, Memory, MemoryType, Module as ReferenceModule, Store};
use wasmtime_wasi::{
    p1::{self, WasiP1Ctx},
    DirPerms, FilePerms, WasiCtxBuilder,
};

const INITIAL_BYTES: &[u8] = b"abcdef";
const FAILED_EXISTING_SENTINEL: u32 = 0xdead_beef;
const FAILED_NEW_SENTINEL: u32 = 0xfeed_face;

#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenFlagsTrace {
    errnos: [i32; 6],
    failed_existing_fd: u32,
    failed_new_fd: u32,
    old_fd_nread: u32,
    seed_bytes: Vec<u8>,
    new_bytes: Vec<u8>,
}

struct IsolatedDirectory {
    path: PathBuf,
}

impl IsolatedDirectory {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mini-wasm-runtime-wasi-open-flags-{}-{id}",
            process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).expect("remove stale path_open flag differential directory");
        }
        fs::create_dir(&path).expect("create isolated path_open flag differential directory");
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
    let read_write = RIGHTS_FD_READ | RIGHTS_FD_WRITE;
    wat::parse_str(format!(
        r#"
        (module
            (import "wasi_snapshot_preview1" "path_open"
                (func $path_open
                    (param i32 i32 i32 i32 i32 i64 i64 i32 i32)
                    (result i32)))
            (import "wasi_snapshot_preview1" "fd_read"
                (func $fd_read (param i32 i32 i32 i32) (result i32)))
            (import "env" "memory" (memory 1 1))
            (export "memory" (memory 0))
            (data (i32.const 0) "seed.bin")
            (data (i32.const 16) "new.bin")
            (data (i32.const 96) "\80\00\00\00\08\00\00\00")

            (func (export "open_existing_excl") (result i32)
                i32.const 3
                i32.const 0
                i32.const 0
                i32.const 8
                i32.const {create_excl}
                i64.const {read}
                i64.const 0
                i32.const 0
                i32.const 64
                call $path_open)

            (func (export "open_old") (result i32)
                i32.const 3
                i32.const 0
                i32.const 0
                i32.const 8
                i32.const 0
                i64.const {read}
                i64.const 0
                i32.const 0
                i32.const 68
                call $path_open)

            (func (export "truncate_existing") (result i32)
                i32.const 3
                i32.const 0
                i32.const 0
                i32.const 8
                i32.const {trunc}
                i64.const {read_write}
                i64.const 0
                i32.const 0
                i32.const 72
                call $path_open)

            (func (export "read_old") (result i32)
                i32.const 68
                i32.load
                i32.const 96
                i32.const 1
                i32.const 108
                call $fd_read)

            (func (export "create_new_excl") (result i32)
                i32.const 3
                i32.const 0
                i32.const 16
                i32.const 7
                i32.const {create_excl}
                i64.const {read_write}
                i64.const 0
                i32.const 0
                i32.const 76
                call $path_open)

            (func (export "open_new_excl_again") (result i32)
                i32.const 3
                i32.const 0
                i32.const 16
                i32.const 7
                i32.const {create_excl}
                i64.const {read}
                i64.const 0
                i32.const 0
                i32.const 80
                call $path_open))
        "#,
        create_excl = OFLAGS_CREAT | OFLAGS_EXCL,
        trunc = OFLAGS_TRUNC,
        read = RIGHTS_FD_READ,
        read_write = read_write,
    ))
    .expect("compile deterministic WASI path_open flag module")
}

fn mini_errno(instance: &mut MiniInstance, export: &str) -> i32 {
    match instance
        .invoke_export(export, &[])
        .unwrap_or_else(|error| panic!("mini path_open flag call {export:?} trapped: {error:?}"))
    {
        Some(Value::I32(errno)) => errno,
        other => panic!("mini path_open flag call {export:?} returned {other:?}"),
    }
}

fn mini_u32(memory: &MemoryHandle, address: u32) -> u32 {
    u32::from_le_bytes(
        memory
            .read(address, 4)
            .expect("read mini path_open flag u32")
            .try_into()
            .expect("fixed u32 width"),
    )
}

fn run_mini(bytes: &[u8]) -> OpenFlagsTrace {
    let memory = MemoryHandle::new(1, Some(1)).expect("create mini path_open flag memory");
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .expect("configure mini path_open flag preopen")
        .with_writable_file("/sandbox", "seed.bin", INITIAL_BYTES)
        .expect("configure mini path_open flag file");
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .expect("register mini path_open flag memory");
    wasi.register(&mut hosts)
        .expect("register mini path_open flag WASI surface");
    let module = parse_module(bytes).expect("path_open flag module must parse in mini runtime");
    let mut instance =
        MiniInstance::with_hosts(module, hosts).expect("path_open flag module must instantiate");

    memory
        .write(64, &FAILED_EXISTING_SENTINEL.to_le_bytes())
        .expect("seed existing EXCL fd sentinel");
    memory
        .write(80, &FAILED_NEW_SENTINEL.to_le_bytes())
        .expect("seed second EXCL fd sentinel");
    memory
        .write(108, &u32::MAX.to_le_bytes())
        .expect("seed old-fd nread sentinel");

    let errnos = [
        mini_errno(&mut instance, "open_existing_excl"),
        mini_errno(&mut instance, "open_old"),
        mini_errno(&mut instance, "truncate_existing"),
        mini_errno(&mut instance, "read_old"),
        mini_errno(&mut instance, "create_new_excl"),
        mini_errno(&mut instance, "open_new_excl_again"),
    ];

    OpenFlagsTrace {
        errnos,
        failed_existing_fd: mini_u32(&memory, 64),
        failed_new_fd: mini_u32(&memory, 80),
        old_fd_nread: mini_u32(&memory, 108),
        seed_bytes: wasi
            .file_snapshot("/sandbox", "seed.bin")
            .expect("snapshot mini truncated seed file"),
        new_bytes: wasi
            .file_snapshot("/sandbox", "new.bin")
            .expect("snapshot mini exclusive-created file"),
    }
}

fn reference_errno(
    instance: &wasmtime::Instance,
    store: &mut Store<WasiP1Ctx>,
    export: &str,
) -> i32 {
    instance
        .get_typed_func::<(), i32>(&mut *store, export)
        .unwrap_or_else(|error| {
            panic!("resolve Wasmtime path_open flag export {export:?}: {error}")
        })
        .call(&mut *store, ())
        .unwrap_or_else(|error| panic!("Wasmtime path_open flag call {export:?} trapped: {error}"))
}

fn reference_u32(memory: Memory, store: &Store<WasiP1Ctx>, address: u32) -> u32 {
    let mut bytes = [0_u8; 4];
    memory
        .read(store, address as usize, &mut bytes)
        .expect("read Wasmtime path_open flag u32");
    u32::from_le_bytes(bytes)
}

fn run_reference(engine: &Engine, bytes: &[u8]) -> OpenFlagsTrace {
    let root = IsolatedDirectory::new();
    let seed_file = root.path().join("seed.bin");
    let new_file = root.path().join("new.bin");
    fs::write(&seed_file, INITIAL_BYTES).expect("seed Wasmtime path_open flag file");

    let module =
        ReferenceModule::new(engine, bytes).expect("compile path_open flag module in Wasmtime");
    let mut builder = WasiCtxBuilder::new();
    builder
        .preopened_dir(root.path(), "/sandbox", DirPerms::all(), FilePerms::all())
        .expect("configure isolated Wasmtime path_open flag preopen");
    let mut store = Store::new(engine, builder.build_p1());
    let memory = Memory::new(&mut store, MemoryType::new(1, Some(1)))
        .expect("create Wasmtime path_open flag memory");
    let mut linker: Linker<WasiP1Ctx> = Linker::new(engine);
    p1::add_to_linker_sync(&mut linker, |context| context)
        .expect("register Wasmtime path_open flag imports");
    linker
        .define(&store, "env", "memory", memory)
        .expect("register Wasmtime path_open flag memory");
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiate path_open flag module in Wasmtime");

    memory
        .write(&mut store, 64, &FAILED_EXISTING_SENTINEL.to_le_bytes())
        .expect("seed Wasmtime existing EXCL fd sentinel");
    memory
        .write(&mut store, 80, &FAILED_NEW_SENTINEL.to_le_bytes())
        .expect("seed Wasmtime second EXCL fd sentinel");
    memory
        .write(&mut store, 108, &u32::MAX.to_le_bytes())
        .expect("seed Wasmtime old-fd nread sentinel");

    let errnos = [
        reference_errno(&instance, &mut store, "open_existing_excl"),
        reference_errno(&instance, &mut store, "open_old"),
        reference_errno(&instance, &mut store, "truncate_existing"),
        reference_errno(&instance, &mut store, "read_old"),
        reference_errno(&instance, &mut store, "create_new_excl"),
        reference_errno(&instance, &mut store, "open_new_excl_again"),
    ];
    let failed_existing_fd = reference_u32(memory, &store, 64);
    let failed_new_fd = reference_u32(memory, &store, 80);
    let old_fd_nread = reference_u32(memory, &store, 108);
    drop(store);

    OpenFlagsTrace {
        errnos,
        failed_existing_fd,
        failed_new_fd,
        old_fd_nread,
        seed_bytes: fs::read(seed_file).expect("read Wasmtime truncated seed file"),
        new_bytes: fs::read(new_file).expect("read Wasmtime exclusive-created file"),
    }
}

fn expected_trace() -> OpenFlagsTrace {
    OpenFlagsTrace {
        errnos: [
            ERRNO_EXIST,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_EXIST,
        ],
        failed_existing_fd: FAILED_EXISTING_SENTINEL,
        failed_new_fd: FAILED_NEW_SENTINEL,
        old_fd_nread: 0,
        seed_bytes: Vec::new(),
        new_bytes: Vec::new(),
    }
}

#[test]
fn path_open_exclusive_and_truncate_lifecycle_matches_wasmtime_wasi() {
    let bytes = module_bytes();
    let mini = run_mini(&bytes);
    let reference = run_reference(&Engine::default(), &bytes);
    let expected = expected_trace();

    assert_eq!(mini, expected, "mini WASI path_open flag trace mismatch");
    assert_eq!(
        reference, expected,
        "Wasmtime WASI path_open flag trace mismatch"
    );
    assert_eq!(mini, reference, "WASI path_open flag differential mismatch");
}
