use std::{
    fs,
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
};

use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance as MiniInstance, MemoryHandle, Value};
use wasm_wasi::{WasiPreview1, ERRNO_SUCCESS, FILETYPE_DIRECTORY, FILETYPE_REGULAR_FILE};
use wasmtime::{Engine, Linker, Memory, MemoryType, Module as ReferenceModule, Store};
use wasmtime_wasi::{
    p1::{self, WasiP1Ctx},
    DirPerms, FilePerms, WasiCtxBuilder,
};

const BUFUSED: u32 = 32;
const BUFFER: u32 = 64;
const BUFFER_LEN: u32 = 512;
const TRUNCATED_LEN: u32 = 26;
const DIRENT_SIZE: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq)]
struct PortableDirent {
    next: u64,
    filetype: u8,
    name: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReaddirTrace {
    errnos: [i32; 3],
    full_names: Vec<Vec<u8>>,
    full_types: Vec<u8>,
    dot_parent_same_inode: bool,
    full_used: u32,
    resume_names: Vec<Vec<u8>>,
    truncated_used: u32,
    truncated_names: Vec<Vec<u8>>,
    truncation_preserved_sentinel: bool,
}

struct IsolatedDirectory {
    path: PathBuf,
}

impl IsolatedDirectory {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mini-wasm-runtime-wasi-readdir-{}-{id}",
            process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).expect("remove stale WASI readdir differential directory");
        }
        fs::create_dir(&path).expect("create isolated WASI readdir differential directory");
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
            (import "wasi_snapshot_preview1" "fd_readdir"
                (func $fd_readdir (param i32 i32 i32 i64 i32) (result i32)))
            (import "env" "memory" (memory 1 1))
            (export "memory" (memory 0))
            (func (export "readdir") (param $cookie i64) (param $buf i32) (param $len i32) (param $used i32) (result i32)
                i32.const 3
                local.get $buf
                local.get $len
                local.get $cookie
                local.get $used
                call $fd_readdir))
        "#,
    )
    .expect("compile deterministic WASI readdir module")
}

fn parse_dirents(bytes: &[u8]) -> Vec<(PortableDirent, u64)> {
    let mut cursor = 0;
    let mut entries = Vec::new();
    while bytes.len().saturating_sub(cursor) >= DIRENT_SIZE {
        let header = &bytes[cursor..cursor + DIRENT_SIZE];
        let next = u64::from_le_bytes(header[0..8].try_into().expect("dirent next"));
        let ino = u64::from_le_bytes(header[8..16].try_into().expect("dirent inode"));
        let name_len =
            u32::from_le_bytes(header[16..20].try_into().expect("dirent name length")) as usize;
        let filetype = header[20];
        let name_start = cursor + DIRENT_SIZE;
        let Some(name_end) = name_start.checked_add(name_len) else {
            break;
        };
        if name_end > bytes.len() {
            break;
        }
        entries.push((
            PortableDirent {
                next,
                filetype,
                name: bytes[name_start..name_end].to_vec(),
            },
            ino,
        ));
        cursor = name_end;
    }
    entries
}

fn mini_errno(instance: &mut MiniInstance, cookie: u64, len: u32) -> i32 {
    match instance
        .invoke_export_values(
            "readdir",
            &[
                Value::I64(cookie as i64),
                Value::I32(BUFFER as i32),
                Value::I32(len as i32),
                Value::I32(BUFUSED as i32),
            ],
        )
        .unwrap_or_else(|error| panic!("mini WASI readdir trapped: {error:?}"))
        .as_slice()
    {
        [Value::I32(errno)] => *errno,
        other => panic!("mini WASI readdir returned {other:?}"),
    }
}

fn mini_u32(memory: &MemoryHandle, address: u32) -> u32 {
    u32::from_le_bytes(
        memory
            .read(address, 4)
            .expect("read mini readdir u32")
            .try_into()
            .expect("fixed u32 width"),
    )
}

fn run_mini(bytes: &[u8]) -> ReaddirTrace {
    let memory = MemoryHandle::new(1, Some(1)).expect("create mini readdir memory");
    let wasi = WasiPreview1::new()
        .with_preopen("/sandbox")
        .expect("configure mini readdir preopen")
        .with_read_only_file("/sandbox", "alpha.txt", b"a")
        .expect("configure mini readdir file");
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .expect("register mini readdir memory");
    wasi.register(&mut hosts)
        .expect("register mini WASI surface");
    let module = parse_module(bytes).expect("readdir module must parse in mini runtime");
    let mut instance = MiniInstance::with_hosts(module, hosts)
        .expect("readdir module must instantiate in mini runtime");

    memory
        .write(BUFFER, &[0x5a; 600])
        .expect("seed mini sentinel");
    let full_errno = mini_errno(&mut instance, 0, BUFFER_LEN);
    let full_used = mini_u32(&memory, BUFUSED);
    let full_raw = memory
        .read(BUFFER, full_used as usize)
        .expect("read mini full readdir payload");
    let full = parse_dirents(&full_raw);
    let resume_cookie = full[1].0.next;
    let dot_parent_same_inode = full[0].1 == full[1].1;

    memory
        .write(BUFFER, &[0x5a; 600])
        .expect("reset mini sentinel");
    let resume_errno = mini_errno(&mut instance, resume_cookie, BUFFER_LEN);
    let resume_used = mini_u32(&memory, BUFUSED);
    let resume = parse_dirents(
        &memory
            .read(BUFFER, resume_used as usize)
            .expect("read mini resumed readdir payload"),
    );

    memory
        .write(BUFFER, &[0x5a; 600])
        .expect("reset mini truncation sentinel");
    let truncated_errno = mini_errno(&mut instance, 0, TRUNCATED_LEN);
    let truncated_used = mini_u32(&memory, BUFUSED);
    let truncated = parse_dirents(
        &memory
            .read(BUFFER, truncated_used as usize)
            .expect("read mini truncated readdir payload"),
    );
    let sentinel = memory
        .read(BUFFER + TRUNCATED_LEN, 16)
        .expect("read mini truncation sentinel");

    ReaddirTrace {
        errnos: [full_errno, resume_errno, truncated_errno],
        full_names: full.iter().map(|(entry, _)| entry.name.clone()).collect(),
        full_types: full.iter().map(|(entry, _)| entry.filetype).collect(),
        dot_parent_same_inode,
        full_used,
        resume_names: resume.iter().map(|(entry, _)| entry.name.clone()).collect(),
        truncated_used,
        truncated_names: truncated
            .iter()
            .map(|(entry, _)| entry.name.clone())
            .collect(),
        truncation_preserved_sentinel: sentinel == vec![0x5a; 16],
    }
}

fn reference_errno(
    instance: &wasmtime::Instance,
    store: &mut Store<WasiP1Ctx>,
    cookie: u64,
    len: u32,
) -> i32 {
    instance
        .get_typed_func::<(i64, i32, i32, i32), i32>(&mut *store, "readdir")
        .expect("resolve Wasmtime readdir export")
        .call(
            &mut *store,
            (cookie as i64, BUFFER as i32, len as i32, BUFUSED as i32),
        )
        .unwrap_or_else(|error| panic!("Wasmtime readdir trapped: {error}"))
}

fn reference_u32(memory: Memory, store: &Store<WasiP1Ctx>, address: u32) -> u32 {
    let mut bytes = [0_u8; 4];
    memory
        .read(store, address as usize, &mut bytes)
        .expect("read Wasmtime readdir u32");
    u32::from_le_bytes(bytes)
}

fn read_reference(memory: Memory, store: &Store<WasiP1Ctx>, address: u32, len: usize) -> Vec<u8> {
    let mut bytes = vec![0; len];
    memory
        .read(store, address as usize, &mut bytes)
        .expect("read Wasmtime readdir bytes");
    bytes
}

fn run_reference(engine: &Engine, bytes: &[u8]) -> ReaddirTrace {
    let root = IsolatedDirectory::new();
    fs::write(root.path().join("alpha.txt"), b"a").expect("seed Wasmtime readdir file");

    let module = ReferenceModule::new(engine, bytes).expect("compile WASI readdir module");
    let mut builder = WasiCtxBuilder::new();
    builder
        .preopened_dir(root.path(), "/sandbox", DirPerms::READ, FilePerms::READ)
        .expect("configure isolated Wasmtime readdir preopen");
    let mut store = Store::new(engine, builder.build_p1());
    let memory = Memory::new(&mut store, MemoryType::new(1, Some(1)))
        .expect("create Wasmtime readdir memory");
    let mut linker: Linker<WasiP1Ctx> = Linker::new(engine);
    p1::add_to_linker_sync(&mut linker, |context| context)
        .expect("register Wasmtime WASI Preview1 readdir imports");
    linker
        .define(&store, "env", "memory", memory)
        .expect("register Wasmtime readdir memory");
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiate readdir module in Wasmtime");

    memory
        .write(&mut store, BUFFER as usize, &[0x5a; 600])
        .expect("seed Wasmtime sentinel");
    let full_errno = reference_errno(&instance, &mut store, 0, BUFFER_LEN);
    let full_used = reference_u32(memory, &store, BUFUSED);
    let full = parse_dirents(&read_reference(memory, &store, BUFFER, full_used as usize));
    let resume_cookie = full[1].0.next;
    let dot_parent_same_inode = full[0].1 == full[1].1;

    memory
        .write(&mut store, BUFFER as usize, &[0x5a; 600])
        .expect("reset Wasmtime sentinel");
    let resume_errno = reference_errno(&instance, &mut store, resume_cookie, BUFFER_LEN);
    let resume_used = reference_u32(memory, &store, BUFUSED);
    let resume = parse_dirents(&read_reference(
        memory,
        &store,
        BUFFER,
        resume_used as usize,
    ));

    memory
        .write(&mut store, BUFFER as usize, &[0x5a; 600])
        .expect("reset Wasmtime truncation sentinel");
    let truncated_errno = reference_errno(&instance, &mut store, 0, TRUNCATED_LEN);
    let truncated_used = reference_u32(memory, &store, BUFUSED);
    let truncated = parse_dirents(&read_reference(
        memory,
        &store,
        BUFFER,
        truncated_used as usize,
    ));
    let sentinel = read_reference(memory, &store, BUFFER + TRUNCATED_LEN, 16);

    ReaddirTrace {
        errnos: [full_errno, resume_errno, truncated_errno],
        full_names: full.iter().map(|(entry, _)| entry.name.clone()).collect(),
        full_types: full.iter().map(|(entry, _)| entry.filetype).collect(),
        dot_parent_same_inode,
        full_used,
        resume_names: resume.iter().map(|(entry, _)| entry.name.clone()).collect(),
        truncated_used,
        truncated_names: truncated
            .iter()
            .map(|(entry, _)| entry.name.clone())
            .collect(),
        truncation_preserved_sentinel: sentinel == vec![0x5a; 16],
    }
}

#[test]
fn flat_preopen_readdir_matches_wasmtime_preview1() {
    let bytes = module_bytes();
    let mini = run_mini(&bytes);
    let reference = run_reference(&Engine::default(), &bytes);

    let expected = ReaddirTrace {
        errnos: [ERRNO_SUCCESS; 3],
        full_names: vec![b".".to_vec(), b"..".to_vec(), b"alpha.txt".to_vec()],
        full_types: vec![
            FILETYPE_DIRECTORY,
            FILETYPE_DIRECTORY,
            FILETYPE_REGULAR_FILE,
        ],
        dot_parent_same_inode: true,
        full_used: 84,
        resume_names: vec![b"alpha.txt".to_vec()],
        truncated_used: TRUNCATED_LEN,
        truncated_names: vec![b".".to_vec()],
        truncation_preserved_sentinel: true,
    };

    assert_eq!(mini, expected, "mini runtime readdir trace drifted");
    assert_eq!(reference, expected, "Wasmtime readdir trace drifted");
}
