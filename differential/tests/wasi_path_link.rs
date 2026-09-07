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

const REOPENED_SOURCE_FD: u32 = 40;
const STAT_BEFORE: u32 = 64;
const STAT_SOURCE_LINKED: u32 = 128;
const STAT_ALIAS_LINKED: u32 = 192;
const STAT_ALIAS_AFTER_SOURCE_UNLINK: u32 = 256;
const STAT_SOURCE_AFTER_FINAL_UNLINK: u32 = 320;
const FILESTAT_INO_OFFSET: u32 = 8;
const FILESTAT_NLINK_OFFSET: u32 = 24;
const FILESTAT_SIZE_OFFSET: u32 = 32;
const PWRITE_IOV: u32 = 400;
const PWRITE_NWRITTEN: u32 = 408;
const WRITE_BYTE: u32 = 416;
const PREAD_IOV: u32 = 424;
const PREAD_NREAD: u32 = 432;
const READ_BYTES: u32 = 440;
const INITIAL_BYTES: &[u8] = b"abc";
const MUTATED_BYTES: &[u8] = b"aZc";

#[derive(Debug, Clone, PartialEq, Eq)]
struct HardLinkTrace {
    errnos: [i32; 13],
    nlinks: [u64; 5],
    sizes: [u64; 5],
    one_inode_lifecycle: bool,
    reopened_source_fd_sentinel: u32,
    nwritten: u32,
    nread: u32,
    linked_source_bytes: Vec<u8>,
    linked_alias_bytes: Vec<u8>,
    source_exists_after_first_unlink: bool,
    alias_exists_after_first_unlink: bool,
    source_exists_after_final_unlink: bool,
    alias_exists_after_final_unlink: bool,
    open_bytes_after_final_unlink: Vec<u8>,
}

struct IsolatedDirectory {
    path: PathBuf,
}

impl IsolatedDirectory {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mini-wasm-runtime-wasi-hard-link-{}-{id}",
            process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).expect("remove stale WASI hard-link differential directory");
        }
        fs::create_dir(&path).expect("create isolated WASI hard-link differential directory");
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
            (data (i32.const 16) "alias.bin")

            (func (export "open_source") (result i32)
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
            (func (export "link") (result i32)
                i32.const 3
                i32.const 0
                i32.const 0
                i32.const 8
                i32.const 3
                i32.const 16
                i32.const 9
                call $path_link)
            (func (export "open_alias") (result i32)
                i32.const 3
                i32.const 0
                i32.const 16
                i32.const 9
                i32.const 0
                i64.const 2097222
                i64.const 0
                i32.const 0
                i32.const 36
                call $path_open)
            (func (export "stat_source_linked") (result i32)
                i32.const 32
                i32.load
                i32.const 128
                call $fd_filestat_get)
            (func (export "stat_alias_linked") (result i32)
                i32.const 36
                i32.load
                i32.const 192
                call $fd_filestat_get)
            (func (export "pwrite_alias") (result i32)
                i32.const 36
                i32.load
                i32.const 400
                i32.const 1
                i64.const 1
                i32.const 408
                call $fd_pwrite)
            (func (export "unlink_source") (result i32)
                i32.const 3
                i32.const 0
                i32.const 8
                call $path_unlink_file)
            (func (export "stat_alias_after_source_unlink") (result i32)
                i32.const 36
                i32.load
                i32.const 256
                call $fd_filestat_get)
            (func (export "reopen_source") (result i32)
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
            (func (export "unlink_alias") (result i32)
                i32.const 3
                i32.const 16
                i32.const 9
                call $path_unlink_file)
            (func (export "stat_source_after_final_unlink") (result i32)
                i32.const 32
                i32.load
                i32.const 320
                call $fd_filestat_get)
            (func (export "pread_source") (result i32)
                i32.const 32
                i32.load
                i32.const 424
                i32.const 1
                i64.const 0
                i32.const 432
                call $fd_pread))
        "#,
    )
    .expect("compile deterministic WASI hard-link module")
}

fn mini_errno(instance: &mut MiniInstance, export: &str) -> i32 {
    match instance
        .invoke_export(export, &[])
        .unwrap_or_else(|error| panic!("mini WASI hard-link call {export:?} trapped: {error:?}"))
    {
        Some(Value::I32(errno)) => errno,
        other => panic!("mini WASI hard-link call {export:?} returned {other:?}"),
    }
}

fn mini_u32(memory: &MemoryHandle, address: u32) -> u32 {
    u32::from_le_bytes(
        memory
            .read(address, 4)
            .expect("read mini WASI hard-link u32")
            .try_into()
            .expect("fixed u32 width"),
    )
}

fn mini_u64(memory: &MemoryHandle, address: u32) -> u64 {
    u64::from_le_bytes(
        memory
            .read(address, 8)
            .expect("read mini WASI hard-link u64")
            .try_into()
            .expect("fixed u64 width"),
    )
}

fn initialize_mini_memory(memory: &MemoryHandle) {
    memory
        .write(REOPENED_SOURCE_FD, &0xfeedface_u32.to_le_bytes())
        .expect("seed mini source reopen sentinel");
    memory
        .write(PWRITE_IOV, &WRITE_BYTE.to_le_bytes())
        .expect("seed mini hard-link pwrite pointer");
    memory
        .write(PWRITE_IOV + 4, &1_u32.to_le_bytes())
        .expect("seed mini hard-link pwrite length");
    memory
        .write(PREAD_IOV, &READ_BYTES.to_le_bytes())
        .expect("seed mini hard-link pread pointer");
    memory
        .write(PREAD_IOV + 4, &3_u32.to_le_bytes())
        .expect("seed mini hard-link pread length");
    memory
        .write(WRITE_BYTE, b"Z")
        .expect("seed mini hard-link pwrite payload");
}

fn mini_stat_values(memory: &MemoryHandle) -> ([u64; 5], [u64; 5], [u64; 5]) {
    let stats = [
        STAT_BEFORE,
        STAT_SOURCE_LINKED,
        STAT_ALIAS_LINKED,
        STAT_ALIAS_AFTER_SOURCE_UNLINK,
        STAT_SOURCE_AFTER_FINAL_UNLINK,
    ];
    let mut inos = [0; 5];
    let mut nlinks = [0; 5];
    let mut sizes = [0; 5];
    for (index, stat) in stats.into_iter().enumerate() {
        inos[index] = mini_u64(memory, stat + FILESTAT_INO_OFFSET);
        nlinks[index] = mini_u64(memory, stat + FILESTAT_NLINK_OFFSET);
        sizes[index] = mini_u64(memory, stat + FILESTAT_SIZE_OFFSET);
    }
    (inos, nlinks, sizes)
}

fn run_mini(bytes: &[u8]) -> HardLinkTrace {
    let memory = MemoryHandle::new(1, Some(1)).expect("create mini WASI hard-link memory");
    initialize_mini_memory(&memory);
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .expect("configure mini writable hard-link preopen")
        .with_writable_file("/sandbox", "data.bin", INITIAL_BYTES)
        .expect("configure mini hard-link source file");
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .expect("register mini WASI hard-link memory");
    wasi.register(&mut hosts)
        .expect("register mini WASI hard-link surface");
    let module = parse_module(bytes).expect("hard-link module must parse in mini runtime");
    let mut instance = MiniInstance::with_hosts(module, hosts)
        .expect("hard-link module must instantiate in mini runtime");

    let mut errnos = [0; 13];
    errnos[0] = mini_errno(&mut instance, "open_source");
    errnos[1] = mini_errno(&mut instance, "stat_before");
    errnos[2] = mini_errno(&mut instance, "link");
    errnos[3] = mini_errno(&mut instance, "open_alias");
    errnos[4] = mini_errno(&mut instance, "stat_source_linked");
    errnos[5] = mini_errno(&mut instance, "stat_alias_linked");
    errnos[6] = mini_errno(&mut instance, "pwrite_alias");
    let linked_source_bytes = wasi
        .file_snapshot("/sandbox", "data.bin")
        .expect("mini hard-link source remains linked after pwrite");
    let linked_alias_bytes = wasi
        .file_snapshot("/sandbox", "alias.bin")
        .expect("mini hard-link alias exists after pwrite");
    errnos[7] = mini_errno(&mut instance, "unlink_source");
    let source_exists_after_first_unlink = wasi.file_snapshot("/sandbox", "data.bin").is_some();
    let alias_exists_after_first_unlink = wasi.file_snapshot("/sandbox", "alias.bin").is_some();
    errnos[8] = mini_errno(&mut instance, "stat_alias_after_source_unlink");
    errnos[9] = mini_errno(&mut instance, "reopen_source");
    errnos[10] = mini_errno(&mut instance, "unlink_alias");
    let source_exists_after_final_unlink = wasi.file_snapshot("/sandbox", "data.bin").is_some();
    let alias_exists_after_final_unlink = wasi.file_snapshot("/sandbox", "alias.bin").is_some();
    errnos[11] = mini_errno(&mut instance, "stat_source_after_final_unlink");
    errnos[12] = mini_errno(&mut instance, "pread_source");

    let (inos, nlinks, sizes) = mini_stat_values(&memory);
    HardLinkTrace {
        errnos,
        nlinks,
        sizes,
        one_inode_lifecycle: inos.iter().all(|ino| *ino == inos[0]),
        reopened_source_fd_sentinel: mini_u32(&memory, REOPENED_SOURCE_FD),
        nwritten: mini_u32(&memory, PWRITE_NWRITTEN),
        nread: mini_u32(&memory, PREAD_NREAD),
        linked_source_bytes,
        linked_alias_bytes,
        source_exists_after_first_unlink,
        alias_exists_after_first_unlink,
        source_exists_after_final_unlink,
        alias_exists_after_final_unlink,
        open_bytes_after_final_unlink: memory
            .read(READ_BYTES, MUTATED_BYTES.len())
            .expect("read mini old-descriptor bytes after final unlink"),
    }
}

fn reference_errno(
    instance: &wasmtime::Instance,
    store: &mut Store<WasiP1Ctx>,
    export: &str,
) -> i32 {
    instance
        .get_typed_func::<(), i32>(&mut *store, export)
        .unwrap_or_else(|error| panic!("resolve Wasmtime hard-link export {export:?}: {error}"))
        .call(&mut *store, ())
        .unwrap_or_else(|error| panic!("Wasmtime hard-link call {export:?} trapped: {error}"))
}

fn reference_u32(memory: Memory, store: &Store<WasiP1Ctx>, address: u32) -> u32 {
    let mut bytes = [0_u8; 4];
    memory
        .read(store, address as usize, &mut bytes)
        .expect("read Wasmtime WASI hard-link u32");
    u32::from_le_bytes(bytes)
}

fn reference_u64(memory: Memory, store: &Store<WasiP1Ctx>, address: u32) -> u64 {
    let mut bytes = [0_u8; 8];
    memory
        .read(store, address as usize, &mut bytes)
        .expect("read Wasmtime WASI hard-link u64");
    u64::from_le_bytes(bytes)
}

fn initialize_reference_memory(memory: Memory, store: &mut Store<WasiP1Ctx>) {
    memory
        .write(
            &mut *store,
            REOPENED_SOURCE_FD as usize,
            &0xfeedface_u32.to_le_bytes(),
        )
        .expect("seed Wasmtime source reopen sentinel");
    memory
        .write(&mut *store, PWRITE_IOV as usize, &WRITE_BYTE.to_le_bytes())
        .expect("seed Wasmtime hard-link pwrite pointer");
    memory
        .write(&mut *store, (PWRITE_IOV + 4) as usize, &1_u32.to_le_bytes())
        .expect("seed Wasmtime hard-link pwrite length");
    memory
        .write(&mut *store, PREAD_IOV as usize, &READ_BYTES.to_le_bytes())
        .expect("seed Wasmtime hard-link pread pointer");
    memory
        .write(&mut *store, (PREAD_IOV + 4) as usize, &3_u32.to_le_bytes())
        .expect("seed Wasmtime hard-link pread length");
    memory
        .write(&mut *store, WRITE_BYTE as usize, b"Z")
        .expect("seed Wasmtime hard-link pwrite payload");
}

fn reference_stat_values(
    memory: Memory,
    store: &Store<WasiP1Ctx>,
) -> ([u64; 5], [u64; 5], [u64; 5]) {
    let stats = [
        STAT_BEFORE,
        STAT_SOURCE_LINKED,
        STAT_ALIAS_LINKED,
        STAT_ALIAS_AFTER_SOURCE_UNLINK,
        STAT_SOURCE_AFTER_FINAL_UNLINK,
    ];
    let mut inos = [0; 5];
    let mut nlinks = [0; 5];
    let mut sizes = [0; 5];
    for (index, stat) in stats.into_iter().enumerate() {
        inos[index] = reference_u64(memory, store, stat + FILESTAT_INO_OFFSET);
        nlinks[index] = reference_u64(memory, store, stat + FILESTAT_NLINK_OFFSET);
        sizes[index] = reference_u64(memory, store, stat + FILESTAT_SIZE_OFFSET);
    }
    (inos, nlinks, sizes)
}

fn run_reference(engine: &Engine, bytes: &[u8]) -> HardLinkTrace {
    let root = IsolatedDirectory::new();
    let host_source = root.path().join("data.bin");
    let host_alias = root.path().join("alias.bin");
    fs::write(&host_source, INITIAL_BYTES).expect("seed Wasmtime WASI hard-link source");

    let module = ReferenceModule::new(engine, bytes).expect("compile WASI hard-link module");
    let mut builder = WasiCtxBuilder::new();
    builder
        .preopened_dir(root.path(), "/sandbox", DirPerms::all(), FilePerms::all())
        .expect("configure isolated Wasmtime hard-link preopen");
    let mut store = Store::new(engine, builder.build_p1());
    let memory = Memory::new(&mut store, MemoryType::new(1, Some(1)))
        .expect("create Wasmtime WASI hard-link memory");
    initialize_reference_memory(memory, &mut store);
    let mut linker: Linker<WasiP1Ctx> = Linker::new(engine);
    p1::add_to_linker_sync(&mut linker, |context| context)
        .expect("register Wasmtime WASI Preview1 hard-link imports");
    linker
        .define(&store, "env", "memory", memory)
        .expect("register Wasmtime hard-link memory");
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiate hard-link module in Wasmtime");

    let mut errnos = [0; 13];
    errnos[0] = reference_errno(&instance, &mut store, "open_source");
    errnos[1] = reference_errno(&instance, &mut store, "stat_before");
    errnos[2] = reference_errno(&instance, &mut store, "link");
    errnos[3] = reference_errno(&instance, &mut store, "open_alias");
    errnos[4] = reference_errno(&instance, &mut store, "stat_source_linked");
    errnos[5] = reference_errno(&instance, &mut store, "stat_alias_linked");
    errnos[6] = reference_errno(&instance, &mut store, "pwrite_alias");
    let linked_source_bytes =
        fs::read(&host_source).expect("read Wasmtime linked source after pwrite");
    let linked_alias_bytes =
        fs::read(&host_alias).expect("read Wasmtime linked alias after pwrite");
    errnos[7] = reference_errno(&instance, &mut store, "unlink_source");
    let source_exists_after_first_unlink = host_source.exists();
    let alias_exists_after_first_unlink = host_alias.exists();
    errnos[8] = reference_errno(&instance, &mut store, "stat_alias_after_source_unlink");
    errnos[9] = reference_errno(&instance, &mut store, "reopen_source");
    errnos[10] = reference_errno(&instance, &mut store, "unlink_alias");
    let source_exists_after_final_unlink = host_source.exists();
    let alias_exists_after_final_unlink = host_alias.exists();
    errnos[11] = reference_errno(&instance, &mut store, "stat_source_after_final_unlink");
    errnos[12] = reference_errno(&instance, &mut store, "pread_source");

    let (inos, nlinks, sizes) = reference_stat_values(memory, &store);
    let mut open_bytes_after_final_unlink = vec![0_u8; MUTATED_BYTES.len()];
    memory
        .read(
            &store,
            READ_BYTES as usize,
            &mut open_bytes_after_final_unlink,
        )
        .expect("read Wasmtime old-descriptor bytes after final unlink");

    HardLinkTrace {
        errnos,
        nlinks,
        sizes,
        one_inode_lifecycle: inos.iter().all(|ino| *ino == inos[0]),
        reopened_source_fd_sentinel: reference_u32(memory, &store, REOPENED_SOURCE_FD),
        nwritten: reference_u32(memory, &store, PWRITE_NWRITTEN),
        nread: reference_u32(memory, &store, PREAD_NREAD),
        linked_source_bytes,
        linked_alias_bytes,
        source_exists_after_first_unlink,
        alias_exists_after_first_unlink,
        source_exists_after_final_unlink,
        alias_exists_after_final_unlink,
        open_bytes_after_final_unlink,
    }
}

fn expected_trace() -> HardLinkTrace {
    HardLinkTrace {
        errnos: [
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_NOENT,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
        ],
        nlinks: [1, 2, 2, 1, 0],
        sizes: [3, 3, 3, 3, 3],
        one_inode_lifecycle: true,
        reopened_source_fd_sentinel: 0xfeedface,
        nwritten: 1,
        nread: 3,
        linked_source_bytes: MUTATED_BYTES.to_vec(),
        linked_alias_bytes: MUTATED_BYTES.to_vec(),
        source_exists_after_first_unlink: false,
        alias_exists_after_first_unlink: true,
        source_exists_after_final_unlink: false,
        alias_exists_after_final_unlink: false,
        open_bytes_after_final_unlink: MUTATED_BYTES.to_vec(),
    }
}

#[test]
fn deterministic_hard_link_lifecycle_matches_wasmtime_wasi() {
    let bytes = module_bytes();
    let mini = run_mini(&bytes);
    let reference = run_reference(&Engine::default(), &bytes);
    let expected = expected_trace();

    assert_eq!(
        mini, expected,
        "mini WASI hard-link lifecycle trace mismatch"
    );
    assert_eq!(
        reference, expected,
        "Wasmtime WASI hard-link lifecycle trace mismatch"
    );
    assert_eq!(
        mini, reference,
        "mini/Wasmtime WASI hard-link lifecycle differential mismatch"
    );
}
