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

const STAT_SOURCE_BEFORE: u32 = 128;
const STAT_TARGET_BEFORE: u32 = 192;
const STAT_SOURCE_LINKED: u32 = 256;
const STAT_SOURCE_AFTER: u32 = 320;
const STAT_DISPLACED_AFTER: u32 = 384;
const STAT_REPLACEMENT: u32 = 448;
const FILESTAT_INO_OFFSET: u32 = 8;
const FILESTAT_NLINK_OFFSET: u32 = 24;
const PREAD_IOV: u32 = 520;
const PREAD_NREAD: u32 = 528;
const READ_BYTES: u32 = 536;
const SOURCE_BYTES: &[u8] = b"SRC";
const DISPLACED_BYTES: &[u8] = b"OLD";

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenameTrace {
    errnos: [i32; 10],
    source_nlinks: [u64; 3],
    displaced_nlinks: [u64; 2],
    moved_inode_lifecycle: bool,
    displaced_inode_lifecycle: bool,
    source_exists: bool,
    alias_exists: bool,
    target_exists: bool,
    alias_bytes: Vec<u8>,
    target_bytes: Vec<u8>,
    nread: u32,
    displaced_open_bytes: Vec<u8>,
}

struct IsolatedDirectory {
    path: PathBuf,
}

impl IsolatedDirectory {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mini-wasm-runtime-wasi-rename-{}-{id}",
            process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).expect("remove stale WASI rename differential directory");
        }
        fs::create_dir(&path).expect("create isolated WASI rename differential directory");
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
            (import "wasi_snapshot_preview1" "path_link"
                (func $path_link (param i32 i32 i32 i32 i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_rename"
                (func $path_rename (param i32 i32 i32 i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_filestat_get"
                (func $fd_filestat_get (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_pread"
                (func $fd_pread (param i32 i32 i32 i64 i32) (result i32)))
            (import "env" "memory" (memory 1 1))
            (export "memory" (memory 0))
            (data (i32.const 0) "source.bin")
            (data (i32.const 16) "alias.bin")
            (data (i32.const 32) "target.bin")

            (func (export "open_source") (result i32)
                i32.const 3 i32.const 0 i32.const 0 i32.const 10 i32.const 0
                i64.const 2097158 i64.const 0 i32.const 0 i32.const 64
                call $path_open)
            (func (export "open_target") (result i32)
                i32.const 3 i32.const 0 i32.const 32 i32.const 10 i32.const 0
                i64.const 2097158 i64.const 0 i32.const 0 i32.const 68
                call $path_open)
            (func (export "stat_source_before") (result i32)
                i32.const 64 i32.load i32.const 128 call $fd_filestat_get)
            (func (export "stat_target_before") (result i32)
                i32.const 68 i32.load i32.const 192 call $fd_filestat_get)
            (func (export "link_alias") (result i32)
                i32.const 3 i32.const 0 i32.const 0 i32.const 10
                i32.const 3 i32.const 16 i32.const 9 call $path_link)
            (func (export "stat_source_linked") (result i32)
                i32.const 64 i32.load i32.const 256 call $fd_filestat_get)
            (func (export "rename_source_over_target") (result i32)
                i32.const 3 i32.const 0 i32.const 10
                i32.const 3 i32.const 32 i32.const 10 call $path_rename)
            (func (export "stat_source_after") (result i32)
                i32.const 64 i32.load i32.const 320 call $fd_filestat_get)
            (func (export "stat_displaced_after") (result i32)
                i32.const 68 i32.load i32.const 384 call $fd_filestat_get)
            (func (export "open_replacement") (result i32)
                i32.const 3 i32.const 0 i32.const 32 i32.const 10 i32.const 0
                i64.const 2097158 i64.const 0 i32.const 0 i32.const 72
                call $path_open)
            (func (export "stat_replacement") (result i32)
                i32.const 72 i32.load i32.const 448 call $fd_filestat_get)
            (func (export "pread_displaced") (result i32)
                i32.const 68 i32.load i32.const 520 i32.const 1 i64.const 0 i32.const 528
                call $fd_pread))
        "#,
    )
    .expect("compile deterministic WASI rename module")
}

fn mini_errno(instance: &mut MiniInstance, export: &str) -> i32 {
    match instance
        .invoke_export(export, &[])
        .unwrap_or_else(|error| panic!("mini WASI rename call {export:?} trapped: {error:?}"))
    {
        Some(Value::I32(errno)) => errno,
        other => panic!("mini WASI rename call {export:?} returned {other:?}"),
    }
}

fn mini_u32(memory: &MemoryHandle, address: u32) -> u32 {
    u32::from_le_bytes(
        memory
            .read(address, 4)
            .expect("read mini WASI rename u32")
            .try_into()
            .expect("fixed u32 width"),
    )
}

fn mini_u64(memory: &MemoryHandle, address: u32) -> u64 {
    u64::from_le_bytes(
        memory
            .read(address, 8)
            .expect("read mini WASI rename u64")
            .try_into()
            .expect("fixed u64 width"),
    )
}

fn initialize_mini_memory(memory: &MemoryHandle) {
    memory
        .write(PREAD_IOV, &READ_BYTES.to_le_bytes())
        .expect("seed mini rename pread pointer");
    memory
        .write(PREAD_IOV + 4, &3_u32.to_le_bytes())
        .expect("seed mini rename pread length");
}

fn run_mini(bytes: &[u8]) -> RenameTrace {
    let memory = MemoryHandle::new(1, Some(1)).expect("create mini WASI rename memory");
    initialize_mini_memory(&memory);
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .expect("configure mini writable rename preopen")
        .with_writable_file("/sandbox", "source.bin", SOURCE_BYTES)
        .expect("configure mini rename source")
        .with_writable_file("/sandbox", "target.bin", DISPLACED_BYTES)
        .expect("configure mini rename target");
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .expect("register mini WASI rename memory");
    wasi.register(&mut hosts)
        .expect("register mini WASI rename surface");
    let module = parse_module(bytes).expect("rename module must parse in mini runtime");
    let mut instance = MiniInstance::with_hosts(module, hosts)
        .expect("rename module must instantiate in mini runtime");

    let mut errnos = [0; 10];
    errnos[0] = mini_errno(&mut instance, "open_source");
    errnos[1] = mini_errno(&mut instance, "open_target");
    errnos[2] = mini_errno(&mut instance, "stat_source_before");
    errnos[3] = mini_errno(&mut instance, "stat_target_before");
    errnos[4] = mini_errno(&mut instance, "link_alias");
    errnos[5] = mini_errno(&mut instance, "stat_source_linked");
    errnos[6] = mini_errno(&mut instance, "rename_source_over_target");
    errnos[7] = mini_errno(&mut instance, "stat_source_after");
    errnos[8] = mini_errno(&mut instance, "stat_displaced_after");
    errnos[9] = mini_errno(&mut instance, "open_replacement");
    assert_eq!(mini_errno(&mut instance, "stat_replacement"), ERRNO_SUCCESS);
    assert_eq!(mini_errno(&mut instance, "pread_displaced"), ERRNO_SUCCESS);

    let source_before_ino = mini_u64(&memory, STAT_SOURCE_BEFORE + FILESTAT_INO_OFFSET);
    let source_linked_ino = mini_u64(&memory, STAT_SOURCE_LINKED + FILESTAT_INO_OFFSET);
    let source_after_ino = mini_u64(&memory, STAT_SOURCE_AFTER + FILESTAT_INO_OFFSET);
    let replacement_ino = mini_u64(&memory, STAT_REPLACEMENT + FILESTAT_INO_OFFSET);
    let displaced_before_ino = mini_u64(&memory, STAT_TARGET_BEFORE + FILESTAT_INO_OFFSET);
    let displaced_after_ino = mini_u64(&memory, STAT_DISPLACED_AFTER + FILESTAT_INO_OFFSET);

    RenameTrace {
        errnos,
        source_nlinks: [
            mini_u64(&memory, STAT_SOURCE_BEFORE + FILESTAT_NLINK_OFFSET),
            mini_u64(&memory, STAT_SOURCE_LINKED + FILESTAT_NLINK_OFFSET),
            mini_u64(&memory, STAT_SOURCE_AFTER + FILESTAT_NLINK_OFFSET),
        ],
        displaced_nlinks: [
            mini_u64(&memory, STAT_TARGET_BEFORE + FILESTAT_NLINK_OFFSET),
            mini_u64(&memory, STAT_DISPLACED_AFTER + FILESTAT_NLINK_OFFSET),
        ],
        moved_inode_lifecycle: source_before_ino == source_linked_ino
            && source_before_ino == source_after_ino
            && source_before_ino == replacement_ino,
        displaced_inode_lifecycle: displaced_before_ino == displaced_after_ino
            && displaced_before_ino != source_before_ino,
        source_exists: wasi.file_snapshot("/sandbox", "source.bin").is_some(),
        alias_exists: wasi.file_snapshot("/sandbox", "alias.bin").is_some(),
        target_exists: wasi.file_snapshot("/sandbox", "target.bin").is_some(),
        alias_bytes: wasi
            .file_snapshot("/sandbox", "alias.bin")
            .expect("mini rename alias survives"),
        target_bytes: wasi
            .file_snapshot("/sandbox", "target.bin")
            .expect("mini rename target points at source"),
        nread: mini_u32(&memory, PREAD_NREAD),
        displaced_open_bytes: memory
            .read(READ_BYTES, DISPLACED_BYTES.len())
            .expect("read mini displaced open handle bytes"),
    }
}

fn reference_errno(
    instance: &wasmtime::Instance,
    store: &mut Store<WasiP1Ctx>,
    export: &str,
) -> i32 {
    instance
        .get_typed_func::<(), i32>(&mut *store, export)
        .unwrap_or_else(|error| panic!("resolve Wasmtime rename export {export:?}: {error}"))
        .call(&mut *store, ())
        .unwrap_or_else(|error| panic!("Wasmtime rename call {export:?} trapped: {error}"))
}

fn reference_u32(memory: Memory, store: &Store<WasiP1Ctx>, address: u32) -> u32 {
    let mut bytes = [0_u8; 4];
    memory
        .read(store, address as usize, &mut bytes)
        .expect("read Wasmtime WASI rename u32");
    u32::from_le_bytes(bytes)
}

fn reference_u64(memory: Memory, store: &Store<WasiP1Ctx>, address: u32) -> u64 {
    let mut bytes = [0_u8; 8];
    memory
        .read(store, address as usize, &mut bytes)
        .expect("read Wasmtime WASI rename u64");
    u64::from_le_bytes(bytes)
}

fn initialize_reference_memory(memory: Memory, store: &mut Store<WasiP1Ctx>) {
    memory
        .write(&mut *store, PREAD_IOV as usize, &READ_BYTES.to_le_bytes())
        .expect("seed Wasmtime rename pread pointer");
    memory
        .write(&mut *store, (PREAD_IOV + 4) as usize, &3_u32.to_le_bytes())
        .expect("seed Wasmtime rename pread length");
}

fn run_reference(engine: &Engine, bytes: &[u8]) -> RenameTrace {
    let root = IsolatedDirectory::new();
    let host_source = root.path().join("source.bin");
    let host_alias = root.path().join("alias.bin");
    let host_target = root.path().join("target.bin");
    fs::write(&host_source, SOURCE_BYTES).expect("seed Wasmtime rename source");
    fs::write(&host_target, DISPLACED_BYTES).expect("seed Wasmtime rename target");

    let module = ReferenceModule::new(engine, bytes).expect("compile WASI rename module");
    let mut builder = WasiCtxBuilder::new();
    builder
        .preopened_dir(root.path(), "/sandbox", DirPerms::all(), FilePerms::all())
        .expect("configure isolated Wasmtime rename preopen");
    let mut store = Store::new(engine, builder.build_p1());
    let memory = Memory::new(&mut store, MemoryType::new(1, Some(1)))
        .expect("create Wasmtime WASI rename memory");
    initialize_reference_memory(memory, &mut store);
    let mut linker: Linker<WasiP1Ctx> = Linker::new(engine);
    p1::add_to_linker_sync(&mut linker, |context| context)
        .expect("register Wasmtime WASI Preview1 rename imports");
    linker
        .define(&store, "env", "memory", memory)
        .expect("register Wasmtime rename memory");
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiate rename module in Wasmtime");

    let mut errnos = [0; 10];
    errnos[0] = reference_errno(&instance, &mut store, "open_source");
    errnos[1] = reference_errno(&instance, &mut store, "open_target");
    errnos[2] = reference_errno(&instance, &mut store, "stat_source_before");
    errnos[3] = reference_errno(&instance, &mut store, "stat_target_before");
    errnos[4] = reference_errno(&instance, &mut store, "link_alias");
    errnos[5] = reference_errno(&instance, &mut store, "stat_source_linked");
    errnos[6] = reference_errno(&instance, &mut store, "rename_source_over_target");
    errnos[7] = reference_errno(&instance, &mut store, "stat_source_after");
    errnos[8] = reference_errno(&instance, &mut store, "stat_displaced_after");
    errnos[9] = reference_errno(&instance, &mut store, "open_replacement");
    assert_eq!(
        reference_errno(&instance, &mut store, "stat_replacement"),
        ERRNO_SUCCESS
    );
    assert_eq!(
        reference_errno(&instance, &mut store, "pread_displaced"),
        ERRNO_SUCCESS
    );

    let source_before_ino = reference_u64(memory, &store, STAT_SOURCE_BEFORE + FILESTAT_INO_OFFSET);
    let source_linked_ino = reference_u64(memory, &store, STAT_SOURCE_LINKED + FILESTAT_INO_OFFSET);
    let source_after_ino = reference_u64(memory, &store, STAT_SOURCE_AFTER + FILESTAT_INO_OFFSET);
    let replacement_ino = reference_u64(memory, &store, STAT_REPLACEMENT + FILESTAT_INO_OFFSET);
    let displaced_before_ino =
        reference_u64(memory, &store, STAT_TARGET_BEFORE + FILESTAT_INO_OFFSET);
    let displaced_after_ino =
        reference_u64(memory, &store, STAT_DISPLACED_AFTER + FILESTAT_INO_OFFSET);
    let mut displaced_open_bytes = vec![0_u8; DISPLACED_BYTES.len()];
    memory
        .read(&store, READ_BYTES as usize, &mut displaced_open_bytes)
        .expect("read Wasmtime displaced open handle bytes");

    RenameTrace {
        errnos,
        source_nlinks: [
            reference_u64(memory, &store, STAT_SOURCE_BEFORE + FILESTAT_NLINK_OFFSET),
            reference_u64(memory, &store, STAT_SOURCE_LINKED + FILESTAT_NLINK_OFFSET),
            reference_u64(memory, &store, STAT_SOURCE_AFTER + FILESTAT_NLINK_OFFSET),
        ],
        displaced_nlinks: [
            reference_u64(memory, &store, STAT_TARGET_BEFORE + FILESTAT_NLINK_OFFSET),
            reference_u64(memory, &store, STAT_DISPLACED_AFTER + FILESTAT_NLINK_OFFSET),
        ],
        moved_inode_lifecycle: source_before_ino == source_linked_ino
            && source_before_ino == source_after_ino
            && source_before_ino == replacement_ino,
        displaced_inode_lifecycle: displaced_before_ino == displaced_after_ino
            && displaced_before_ino != source_before_ino,
        source_exists: host_source.exists(),
        alias_exists: host_alias.exists(),
        target_exists: host_target.exists(),
        alias_bytes: fs::read(&host_alias).expect("read Wasmtime rename alias"),
        target_bytes: fs::read(&host_target).expect("read Wasmtime renamed target"),
        nread: reference_u32(memory, &store, PREAD_NREAD),
        displaced_open_bytes,
    }
}

fn expected_trace() -> RenameTrace {
    RenameTrace {
        errnos: [ERRNO_SUCCESS; 10],
        source_nlinks: [1, 2, 2],
        displaced_nlinks: [1, 0],
        moved_inode_lifecycle: true,
        displaced_inode_lifecycle: true,
        source_exists: false,
        alias_exists: true,
        target_exists: true,
        alias_bytes: SOURCE_BYTES.to_vec(),
        target_bytes: SOURCE_BYTES.to_vec(),
        nread: 3,
        displaced_open_bytes: DISPLACED_BYTES.to_vec(),
    }
}

#[test]
fn path_rename_replacement_matches_pinned_wasmtime_wasi() {
    let bytes = module_bytes();
    let expected = expected_trace();
    let mini = run_mini(&bytes);
    let reference = run_reference(&Engine::default(), &bytes);

    assert_eq!(mini, expected, "mini runtime rename lifecycle drifted");
    assert_eq!(reference, expected, "Wasmtime rename lifecycle drifted");
    assert_eq!(mini, reference, "mini/Wasmtime rename lifecycle mismatch");
}
