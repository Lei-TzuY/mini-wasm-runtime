use std::{
    fs,
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
};

use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance as MiniInstance, MemoryHandle, Value};
use wasm_wasi::{WasiPreview1, ERRNO_NOENT, ERRNO_SUCCESS, FILETYPE_SYMBOLIC_LINK};
use wasmtime::{Engine, Linker, Memory, MemoryType, Module as ReferenceModule, Store};
use wasmtime_wasi::{
    p1::{self, WasiP1Ctx},
    DirPerms, FilePerms, WasiCtxBuilder,
};

const EXACT_USED: u32 = 48;
const TRUNC_USED: u32 = 52;
const READDIR_USED: u32 = 56;
const AFTER_UNLINK_USED: u32 = 60;
const EXACT_BUF: u32 = 64;
const TRUNC_BUF: u32 = 128;
const READDIR_BUF: u32 = 256;
const READDIR_LEN: u32 = 512;
const AFTER_UNLINK_BUF: u32 = 800;
const DIRENT_SIZE: usize = 24;
const TARGET: &[u8] = b"../outside/data.bin";
const LINK_NAME: &[u8] = b"shortcut";
const USED_SENTINEL: u32 = 0xfeed_face;

#[derive(Debug, Clone, PartialEq, Eq)]
struct SymlinkTrace {
    errnos: [i32; 6],
    exact_used: u32,
    exact_bytes: Vec<u8>,
    exact_adjacent: u8,
    trunc_used: u32,
    trunc_bytes: Vec<u8>,
    trunc_adjacent: u8,
    readdir_link_type: Option<u8>,
    after_unlink_used: u32,
}

struct IsolatedDirectory {
    path: PathBuf,
}

impl IsolatedDirectory {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mini-wasm-runtime-wasi-symlink-{}-{id}",
            process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).expect("remove stale symlink differential root");
        }
        fs::create_dir(&path).expect("create isolated symlink differential root");
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
            (import "wasi_snapshot_preview1" "path_symlink"
                (func $path_symlink (param i32 i32 i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_readlink"
                (func $path_readlink (param i32 i32 i32 i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_readdir"
                (func $fd_readdir (param i32 i32 i32 i64 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_unlink_file"
                (func $path_unlink_file (param i32 i32 i32) (result i32)))
            (import "env" "memory" (memory 1 1))
            (export "memory" (memory 0))
            (data (i32.const 0) "../outside/data.bin")
            (data (i32.const 32) "shortcut")

            (func (export "symlink") (result i32)
                i32.const 0
                i32.const 19
                i32.const 3
                i32.const 32
                i32.const 8
                call $path_symlink)
            (func (export "read_exact") (result i32)
                i32.const 3
                i32.const 32
                i32.const 8
                i32.const 64
                i32.const 64
                i32.const 48
                call $path_readlink)
            (func (export "read_trunc") (result i32)
                i32.const 3
                i32.const 32
                i32.const 8
                i32.const 128
                i32.const 5
                i32.const 52
                call $path_readlink)
            (func (export "readdir") (result i32)
                i32.const 3
                i32.const 256
                i32.const 512
                i64.const 0
                i32.const 56
                call $fd_readdir)
            (func (export "unlink") (result i32)
                i32.const 3
                i32.const 32
                i32.const 8
                call $path_unlink_file)
            (func (export "read_after_unlink") (result i32)
                i32.const 3
                i32.const 32
                i32.const 8
                i32.const 800
                i32.const 64
                i32.const 60
                call $path_readlink)
        )
        "#,
    )
    .expect("compile deterministic WASI symlink module")
}

fn dirent_type_for(bytes: &[u8], name: &[u8]) -> Option<u8> {
    let mut cursor = 0;
    while bytes.len().saturating_sub(cursor) >= DIRENT_SIZE {
        let header = &bytes[cursor..cursor + DIRENT_SIZE];
        let name_len =
            u32::from_le_bytes(header[16..20].try_into().expect("dirent name length")) as usize;
        let start = cursor + DIRENT_SIZE;
        let end = start.checked_add(name_len)?;
        if end > bytes.len() {
            return None;
        }
        if &bytes[start..end] == name {
            return Some(header[20]);
        }
        cursor = end;
    }
    None
}

fn mini_errno(instance: &mut MiniInstance, export: &str) -> i32 {
    match instance
        .invoke_export(export, &[])
        .unwrap_or_else(|error| panic!("mini symlink call {export:?} trapped: {error:?}"))
    {
        Some(Value::I32(errno)) => errno,
        other => panic!("mini symlink call {export:?} returned {other:?}"),
    }
}

fn mini_u32(memory: &MemoryHandle, address: u32) -> u32 {
    u32::from_le_bytes(
        memory
            .read(address, 4)
            .expect("read mini symlink u32")
            .try_into()
            .expect("fixed u32 width"),
    )
}

fn initialize_mini_memory(memory: &MemoryHandle) {
    memory
        .write(EXACT_BUF, &[0xa5; 64])
        .expect("seed mini exact readlink buffer");
    memory
        .write(TRUNC_BUF, &[0xcc; 16])
        .expect("seed mini truncated readlink buffer");
    memory
        .write(READDIR_BUF, &[0x5a; READDIR_LEN as usize])
        .expect("seed mini readdir buffer");
    memory
        .write(AFTER_UNLINK_BUF, &[0x77; 64])
        .expect("seed mini post-unlink readlink buffer");
    memory
        .write(AFTER_UNLINK_USED, &USED_SENTINEL.to_le_bytes())
        .expect("seed mini post-unlink used sentinel");
}

fn run_mini(bytes: &[u8]) -> SymlinkTrace {
    let memory = MemoryHandle::new(1, Some(1)).expect("create mini symlink memory");
    initialize_mini_memory(&memory);
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .expect("configure mini writable preopen");
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .expect("register mini symlink memory");
    wasi.register(&mut hosts)
        .expect("register mini WASI symlink surface");
    let module = parse_module(bytes).expect("symlink module must parse in mini runtime");
    let mut instance = MiniInstance::with_hosts(module, hosts)
        .expect("symlink module must instantiate in mini runtime");

    let symlink_errno = mini_errno(&mut instance, "symlink");
    let exact_errno = mini_errno(&mut instance, "read_exact");
    let trunc_errno = mini_errno(&mut instance, "read_trunc");
    let readdir_errno = mini_errno(&mut instance, "readdir");
    let readdir_used = mini_u32(&memory, READDIR_USED) as usize;
    let readdir_bytes = memory
        .read(READDIR_BUF, readdir_used)
        .expect("read mini symlink dirents");
    let unlink_errno = mini_errno(&mut instance, "unlink");
    let after_unlink_errno = mini_errno(&mut instance, "read_after_unlink");

    SymlinkTrace {
        errnos: [
            symlink_errno,
            exact_errno,
            trunc_errno,
            readdir_errno,
            unlink_errno,
            after_unlink_errno,
        ],
        exact_used: mini_u32(&memory, EXACT_USED),
        exact_bytes: memory
            .read(EXACT_BUF, TARGET.len())
            .expect("read mini exact target bytes"),
        exact_adjacent: memory
            .read(EXACT_BUF + TARGET.len() as u32, 1)
            .expect("read mini exact adjacent byte")[0],
        trunc_used: mini_u32(&memory, TRUNC_USED),
        trunc_bytes: memory
            .read(TRUNC_BUF, 5)
            .expect("read mini truncated target bytes"),
        trunc_adjacent: memory
            .read(TRUNC_BUF + 5, 1)
            .expect("read mini trunc adjacent byte")[0],
        readdir_link_type: dirent_type_for(&readdir_bytes, LINK_NAME),
        after_unlink_used: mini_u32(&memory, AFTER_UNLINK_USED),
    }
}

fn reference_errno(
    instance: &wasmtime::Instance,
    store: &mut Store<WasiP1Ctx>,
    export: &str,
) -> i32 {
    instance
        .get_typed_func::<(), i32>(&mut *store, export)
        .unwrap_or_else(|error| panic!("resolve Wasmtime symlink export {export:?}: {error}"))
        .call(&mut *store, ())
        .unwrap_or_else(|error| panic!("Wasmtime symlink call {export:?} trapped: {error}"))
}

fn reference_u32(memory: Memory, store: &Store<WasiP1Ctx>, address: u32) -> u32 {
    let mut bytes = [0_u8; 4];
    memory
        .read(store, address as usize, &mut bytes)
        .expect("read Wasmtime symlink u32");
    u32::from_le_bytes(bytes)
}

fn initialize_reference_memory(memory: Memory, store: &mut Store<WasiP1Ctx>) {
    memory
        .write(&mut *store, EXACT_BUF as usize, &[0xa5; 64])
        .expect("seed Wasmtime exact readlink buffer");
    memory
        .write(&mut *store, TRUNC_BUF as usize, &[0xcc; 16])
        .expect("seed Wasmtime trunc readlink buffer");
    memory
        .write(
            &mut *store,
            READDIR_BUF as usize,
            &[0x5a; READDIR_LEN as usize],
        )
        .expect("seed Wasmtime readdir buffer");
    memory
        .write(&mut *store, AFTER_UNLINK_BUF as usize, &[0x77; 64])
        .expect("seed Wasmtime post-unlink readlink buffer");
    memory
        .write(
            &mut *store,
            AFTER_UNLINK_USED as usize,
            &USED_SENTINEL.to_le_bytes(),
        )
        .expect("seed Wasmtime post-unlink used sentinel");
}

fn run_reference(engine: &Engine, bytes: &[u8]) -> SymlinkTrace {
    let root = IsolatedDirectory::new();
    let module = ReferenceModule::new(engine, bytes).expect("compile WASI symlink module");
    let mut builder = WasiCtxBuilder::new();
    builder
        .preopened_dir(root.path(), "/sandbox", DirPerms::all(), FilePerms::all())
        .expect("configure isolated Wasmtime symlink preopen");
    let mut store = Store::new(engine, builder.build_p1());
    let memory = Memory::new(&mut store, MemoryType::new(1, Some(1)))
        .expect("create Wasmtime symlink memory");
    initialize_reference_memory(memory, &mut store);
    let mut linker: Linker<WasiP1Ctx> = Linker::new(engine);
    p1::add_to_linker_sync(&mut linker, |context| context)
        .expect("register Wasmtime WASI Preview1 symlink imports");
    linker
        .define(&store, "env", "memory", memory)
        .expect("register Wasmtime symlink memory");
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiate symlink module in Wasmtime");

    let symlink_errno = reference_errno(&instance, &mut store, "symlink");
    let exact_errno = reference_errno(&instance, &mut store, "read_exact");
    let trunc_errno = reference_errno(&instance, &mut store, "read_trunc");
    let readdir_errno = reference_errno(&instance, &mut store, "readdir");
    let readdir_used = reference_u32(memory, &store, READDIR_USED) as usize;
    let mut readdir_bytes = vec![0_u8; readdir_used];
    memory
        .read(&store, READDIR_BUF as usize, &mut readdir_bytes)
        .expect("read Wasmtime symlink dirents");
    let unlink_errno = reference_errno(&instance, &mut store, "unlink");
    let after_unlink_errno = reference_errno(&instance, &mut store, "read_after_unlink");

    let mut exact_bytes = vec![0_u8; TARGET.len()];
    memory
        .read(&store, EXACT_BUF as usize, &mut exact_bytes)
        .expect("read Wasmtime exact target bytes");
    let mut trunc_bytes = vec![0_u8; 5];
    memory
        .read(&store, TRUNC_BUF as usize, &mut trunc_bytes)
        .expect("read Wasmtime truncated target bytes");
    let mut exact_adjacent = [0_u8; 1];
    memory
        .read(
            &store,
            (EXACT_BUF + TARGET.len() as u32) as usize,
            &mut exact_adjacent,
        )
        .expect("read Wasmtime exact adjacent byte");
    let mut trunc_adjacent = [0_u8; 1];
    memory
        .read(&store, (TRUNC_BUF + 5) as usize, &mut trunc_adjacent)
        .expect("read Wasmtime trunc adjacent byte");

    SymlinkTrace {
        errnos: [
            symlink_errno,
            exact_errno,
            trunc_errno,
            readdir_errno,
            unlink_errno,
            after_unlink_errno,
        ],
        exact_used: reference_u32(memory, &store, EXACT_USED),
        exact_bytes,
        exact_adjacent: exact_adjacent[0],
        trunc_used: reference_u32(memory, &store, TRUNC_USED),
        trunc_bytes,
        trunc_adjacent: trunc_adjacent[0],
        readdir_link_type: dirent_type_for(&readdir_bytes, LINK_NAME),
        after_unlink_used: reference_u32(memory, &store, AFTER_UNLINK_USED),
    }
}

fn expected_trace() -> SymlinkTrace {
    SymlinkTrace {
        errnos: [
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_NOENT,
        ],
        exact_used: TARGET.len() as u32,
        exact_bytes: TARGET.to_vec(),
        exact_adjacent: 0xa5,
        trunc_used: 5,
        trunc_bytes: TARGET[..5].to_vec(),
        trunc_adjacent: 0xcc,
        readdir_link_type: Some(FILETYPE_SYMBOLIC_LINK),
        after_unlink_used: USED_SENTINEL,
    }
}

#[test]
fn deterministic_symlink_readlink_readdir_unlink_matches_wasmtime_wasi() {
    let bytes = module_bytes();
    let mini = run_mini(&bytes);
    let reference = run_reference(&Engine::default(), &bytes);
    let expected = expected_trace();
    assert_eq!(mini, expected, "mini symlink lifecycle must match contract");
    assert_eq!(
        reference, expected,
        "Wasmtime-WASI 37.0.3 symlink lifecycle must match portable contract"
    );
    assert_eq!(mini, reference, "mini and Wasmtime symlink traces diverged");
}
