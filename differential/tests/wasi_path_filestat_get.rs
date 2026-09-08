use std::{
    fs,
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
};

use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance as MiniInstance, MemoryHandle, Value};
use wasm_wasi::{
    WasiPreview1, ERRNO_SUCCESS, FILETYPE_DIRECTORY, FILETYPE_REGULAR_FILE, FILETYPE_SYMBOLIC_LINK,
    OFLAGS_CREAT, OFLAGS_DIRECTORY, RIGHTS_FD_READDIR, RIGHTS_PATH_CREATE_FILE,
    RIGHTS_PATH_FILESTAT_GET, RIGHTS_PATH_OPEN,
};
use wasmtime::{
    Engine, Linker, Memory, MemoryType, Module as ReferenceModule, Store, Val as ReferenceValue,
};
use wasmtime_wasi::{
    p1::{self, WasiP1Ctx},
    DirPerms, FilePerms, WasiCtxBuilder,
};

const DATA_PTR: u32 = 1024;
const ALIAS_PTR: u32 = 1040;
const DOCS_PTR: u32 = 1060;
const LINK_PTR: u32 = 1080;
const TARGET_PTR: u32 = 1100;
const NOTE_PTR: u32 = 1120;
const DATA_LEN: u32 = 8;
const ALIAS_LEN: u32 = 9;
const DOCS_LEN: u32 = 4;
const LINK_LEN: u32 = 8;
const TARGET_LEN: u32 = 8;
const NOTE_LEN: u32 = 8;
const STAT0: u32 = 128;
const STAT1: u32 = 192;
const STAT2: u32 = 256;
const STAT3: u32 = 320;
const STAT4: u32 = 384;
const STAT5: u32 = 448;
const DIRECTORY_FD_OUT: u32 = 64;
const CHILD_FD_OUT: u32 = 68;
const DIRECTORY_RIGHTS: u64 =
    RIGHTS_FD_READDIR | RIGHTS_PATH_OPEN | RIGHTS_PATH_CREATE_FILE | RIGHTS_PATH_FILESTAT_GET;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RawStat {
    ino: u64,
    filetype: u8,
    nlink: u64,
    size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PortableStat {
    filetype: u8,
    nlink: u64,
    size: u64,
}

impl From<RawStat> for PortableStat {
    fn from(stat: RawStat) -> Self {
        Self {
            filetype: stat.filetype,
            nlink: stat.nlink,
            size: stat.size,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PathFilestatTrace {
    errnos: [i32; 11],
    regular_before: PortableStat,
    regular_after_link: PortableStat,
    alias_after_link: PortableStat,
    original_inode_stable: bool,
    linked_inode_equal: bool,
    directory_filetype: u8,
    symlink_filetype: u8,
    symlink_size: u64,
    child_filetype: u8,
    child_size: u64,
}

struct IsolatedDirectory {
    path: PathBuf,
}

impl IsolatedDirectory {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mini-wasm-runtime-wasi-path-filestat-{}-{id}",
            process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).expect("remove stale path-filestat differential root");
        }
        fs::create_dir(&path).expect("create isolated path-filestat differential root");
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
            (import "wasi_snapshot_preview1" "path_filestat_get"
                (func $path_filestat_get (param i32 i32 i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_link"
                (func $path_link (param i32 i32 i32 i32 i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_create_directory"
                (func $path_create_directory (param i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_symlink"
                (func $path_symlink (param i32 i32 i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_open"
                (func $path_open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
            (import "env" "memory" (memory 1 1))
            (export "memory" (memory 0))
            (data (i32.const 1024) "data.bin")
            (data (i32.const 1040) "alias.bin")
            (data (i32.const 1060) "docs")
            (data (i32.const 1080) "shortcut")
            (data (i32.const 1100) "data.bin")
            (data (i32.const 1120) "note.txt")

            (func (export "stat")
                (param $fd i32) (param $flags i32) (param $path i32) (param $len i32)
                (param $out i32) (result i32)
                local.get $fd
                local.get $flags
                local.get $path
                local.get $len
                local.get $out
                call $path_filestat_get)
            (func (export "link")
                (param $old_fd i32) (param $old_path i32) (param $old_len i32)
                (param $new_fd i32) (param $new_path i32) (param $new_len i32) (result i32)
                local.get $old_fd
                i32.const 0
                local.get $old_path
                local.get $old_len
                local.get $new_fd
                local.get $new_path
                local.get $new_len
                call $path_link)
            (func (export "mkdir")
                (param $fd i32) (param $path i32) (param $len i32) (result i32)
                local.get $fd local.get $path local.get $len call $path_create_directory)
            (func (export "symlink")
                (param $target i32) (param $target_len i32) (param $fd i32)
                (param $link i32) (param $link_len i32) (result i32)
                local.get $target
                local.get $target_len
                local.get $fd
                local.get $link
                local.get $link_len
                call $path_symlink)
            (func (export "open")
                (param $fd i32) (param $path i32) (param $len i32) (param $oflags i32)
                (param $rights i64) (param $out i32) (result i32)
                local.get $fd
                i32.const 0
                local.get $path
                local.get $len
                local.get $oflags
                local.get $rights
                i64.const 0
                i32.const 0
                local.get $out
                call $path_open)
        )
        "#,
    )
    .expect("compile deterministic WASI path-filestat module")
}

fn decode_stat(bytes: &[u8]) -> RawStat {
    assert_eq!(bytes.len(), 64, "filestat ABI width");
    RawStat {
        ino: u64::from_le_bytes(bytes[8..16].try_into().expect("inode bytes")),
        filetype: bytes[16],
        nlink: u64::from_le_bytes(bytes[24..32].try_into().expect("nlink bytes")),
        size: u64::from_le_bytes(bytes[32..40].try_into().expect("size bytes")),
    }
}

fn mini_call(instance: &mut MiniInstance, name: &str, args: &[Value]) -> i32 {
    match instance
        .invoke_export_values(name, args)
        .unwrap_or_else(|error| panic!("mini path-filestat call {name:?} trapped: {error:?}"))
        .as_slice()
    {
        [Value::I32(errno)] => *errno,
        other => panic!("mini path-filestat call {name:?} returned {other:?}"),
    }
}

fn mini_u32(memory: &MemoryHandle, address: u32) -> u32 {
    u32::from_le_bytes(
        memory
            .read(address, 4)
            .expect("read mini path-filestat u32")
            .try_into()
            .expect("fixed u32 width"),
    )
}

fn mini_stat(memory: &MemoryHandle, address: u32) -> RawStat {
    decode_stat(
        &memory
            .read(address, 64)
            .expect("read mini path-filestat payload"),
    )
}

fn run_mini(bytes: &[u8]) -> PathFilestatTrace {
    let memory = MemoryHandle::new(1, Some(1)).expect("create mini path-filestat memory");
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .expect("configure mini writable preopen")
        .with_writable_file("/sandbox", "data.bin", b"abc")
        .expect("mount mini path-filestat file");
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .expect("register mini path-filestat memory");
    wasi.register(&mut hosts)
        .expect("register mini WASI path-filestat surface");
    let module = parse_module(bytes).expect("path-filestat module parses in mini runtime");
    let mut instance = MiniInstance::with_hosts(module, hosts)
        .expect("path-filestat module instantiates in mini runtime");

    let stat_before_errno = mini_call(
        &mut instance,
        "stat",
        &[
            Value::I32(3),
            Value::I32(0),
            Value::I32(DATA_PTR as i32),
            Value::I32(DATA_LEN as i32),
            Value::I32(STAT0 as i32),
        ],
    );
    let regular_before = mini_stat(&memory, STAT0);
    let link_errno = mini_call(
        &mut instance,
        "link",
        &[
            Value::I32(3),
            Value::I32(DATA_PTR as i32),
            Value::I32(DATA_LEN as i32),
            Value::I32(3),
            Value::I32(ALIAS_PTR as i32),
            Value::I32(ALIAS_LEN as i32),
        ],
    );
    let stat_after_errno = mini_call(
        &mut instance,
        "stat",
        &[
            Value::I32(3),
            Value::I32(0),
            Value::I32(DATA_PTR as i32),
            Value::I32(DATA_LEN as i32),
            Value::I32(STAT1 as i32),
        ],
    );
    let regular_after = mini_stat(&memory, STAT1);
    let alias_stat_errno = mini_call(
        &mut instance,
        "stat",
        &[
            Value::I32(3),
            Value::I32(0),
            Value::I32(ALIAS_PTR as i32),
            Value::I32(ALIAS_LEN as i32),
            Value::I32(STAT2 as i32),
        ],
    );
    let alias_after = mini_stat(&memory, STAT2);
    let mkdir_errno = mini_call(
        &mut instance,
        "mkdir",
        &[
            Value::I32(3),
            Value::I32(DOCS_PTR as i32),
            Value::I32(DOCS_LEN as i32),
        ],
    );
    let dir_stat_errno = mini_call(
        &mut instance,
        "stat",
        &[
            Value::I32(3),
            Value::I32(0),
            Value::I32(DOCS_PTR as i32),
            Value::I32(DOCS_LEN as i32),
            Value::I32(STAT3 as i32),
        ],
    );
    let directory_stat = mini_stat(&memory, STAT3);
    let symlink_errno = mini_call(
        &mut instance,
        "symlink",
        &[
            Value::I32(TARGET_PTR as i32),
            Value::I32(TARGET_LEN as i32),
            Value::I32(3),
            Value::I32(LINK_PTR as i32),
            Value::I32(LINK_LEN as i32),
        ],
    );
    let symlink_stat_errno = mini_call(
        &mut instance,
        "stat",
        &[
            Value::I32(3),
            Value::I32(0),
            Value::I32(LINK_PTR as i32),
            Value::I32(LINK_LEN as i32),
            Value::I32(STAT4 as i32),
        ],
    );
    let symlink_stat = mini_stat(&memory, STAT4);
    let open_dir_errno = mini_call(
        &mut instance,
        "open",
        &[
            Value::I32(3),
            Value::I32(DOCS_PTR as i32),
            Value::I32(DOCS_LEN as i32),
            Value::I32(OFLAGS_DIRECTORY as i32),
            Value::I64(DIRECTORY_RIGHTS as i64),
            Value::I32(DIRECTORY_FD_OUT as i32),
        ],
    );
    let directory_fd = mini_u32(&memory, DIRECTORY_FD_OUT);
    let create_child_errno = mini_call(
        &mut instance,
        "open",
        &[
            Value::I32(directory_fd as i32),
            Value::I32(NOTE_PTR as i32),
            Value::I32(NOTE_LEN as i32),
            Value::I32(OFLAGS_CREAT as i32),
            Value::I64(0),
            Value::I32(CHILD_FD_OUT as i32),
        ],
    );
    let child_stat_errno = mini_call(
        &mut instance,
        "stat",
        &[
            Value::I32(directory_fd as i32),
            Value::I32(0),
            Value::I32(NOTE_PTR as i32),
            Value::I32(NOTE_LEN as i32),
            Value::I32(STAT5 as i32),
        ],
    );
    let child_stat = mini_stat(&memory, STAT5);

    PathFilestatTrace {
        errnos: [
            stat_before_errno,
            link_errno,
            stat_after_errno,
            alias_stat_errno,
            mkdir_errno,
            dir_stat_errno,
            symlink_errno,
            symlink_stat_errno,
            open_dir_errno,
            create_child_errno,
            child_stat_errno,
        ],
        regular_before: regular_before.into(),
        regular_after_link: regular_after.into(),
        alias_after_link: alias_after.into(),
        original_inode_stable: regular_before.ino == regular_after.ino,
        linked_inode_equal: regular_after.ino == alias_after.ino,
        directory_filetype: directory_stat.filetype,
        symlink_filetype: symlink_stat.filetype,
        symlink_size: symlink_stat.size,
        child_filetype: child_stat.filetype,
        child_size: child_stat.size,
    }
}

fn reference_call(
    instance: &wasmtime::Instance,
    store: &mut Store<WasiP1Ctx>,
    name: &str,
    args: &[ReferenceValue],
) -> i32 {
    let function = instance
        .get_func(&mut *store, name)
        .unwrap_or_else(|| panic!("resolve Wasmtime path-filestat export {name:?}"));
    let mut results = [ReferenceValue::I32(0)];
    function
        .call(&mut *store, args, &mut results)
        .unwrap_or_else(|error| panic!("Wasmtime path-filestat call {name:?} trapped: {error}"));
    match results[0] {
        ReferenceValue::I32(errno) => errno,
        ref other => panic!("Wasmtime path-filestat call {name:?} returned {other:?}"),
    }
}

fn reference_u32(memory: Memory, store: &Store<WasiP1Ctx>, address: u32) -> u32 {
    let mut bytes = [0_u8; 4];
    memory
        .read(store, address as usize, &mut bytes)
        .expect("read Wasmtime path-filestat u32");
    u32::from_le_bytes(bytes)
}

fn reference_stat(memory: Memory, store: &Store<WasiP1Ctx>, address: u32) -> RawStat {
    let mut bytes = [0_u8; 64];
    memory
        .read(store, address as usize, &mut bytes)
        .expect("read Wasmtime path-filestat payload");
    decode_stat(&bytes)
}

fn run_reference(engine: &Engine, bytes: &[u8]) -> PathFilestatTrace {
    let root = IsolatedDirectory::new();
    fs::write(root.path().join("data.bin"), b"abc").expect("seed Wasmtime path-filestat file");
    let module = ReferenceModule::new(engine, bytes).expect("compile WASI path-filestat module");
    let mut builder = WasiCtxBuilder::new();
    builder
        .preopened_dir(root.path(), "/sandbox", DirPerms::all(), FilePerms::all())
        .expect("configure isolated Wasmtime path-filestat preopen");
    let mut store = Store::new(engine, builder.build_p1());
    let memory = Memory::new(&mut store, MemoryType::new(1, Some(1)))
        .expect("create Wasmtime path-filestat memory");
    let mut linker: Linker<WasiP1Ctx> = Linker::new(engine);
    p1::add_to_linker_sync(&mut linker, |context| context)
        .expect("register Wasmtime WASI Preview1 path-filestat imports");
    linker
        .define(&store, "env", "memory", memory)
        .expect("register Wasmtime path-filestat memory");
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiate path-filestat module in Wasmtime");

    let stat_before_errno = reference_call(
        &instance,
        &mut store,
        "stat",
        &[
            ReferenceValue::I32(3),
            ReferenceValue::I32(0),
            ReferenceValue::I32(DATA_PTR as i32),
            ReferenceValue::I32(DATA_LEN as i32),
            ReferenceValue::I32(STAT0 as i32),
        ],
    );
    let regular_before = reference_stat(memory, &store, STAT0);
    let link_errno = reference_call(
        &instance,
        &mut store,
        "link",
        &[
            ReferenceValue::I32(3),
            ReferenceValue::I32(DATA_PTR as i32),
            ReferenceValue::I32(DATA_LEN as i32),
            ReferenceValue::I32(3),
            ReferenceValue::I32(ALIAS_PTR as i32),
            ReferenceValue::I32(ALIAS_LEN as i32),
        ],
    );
    let stat_after_errno = reference_call(
        &instance,
        &mut store,
        "stat",
        &[
            ReferenceValue::I32(3),
            ReferenceValue::I32(0),
            ReferenceValue::I32(DATA_PTR as i32),
            ReferenceValue::I32(DATA_LEN as i32),
            ReferenceValue::I32(STAT1 as i32),
        ],
    );
    let regular_after = reference_stat(memory, &store, STAT1);
    let alias_stat_errno = reference_call(
        &instance,
        &mut store,
        "stat",
        &[
            ReferenceValue::I32(3),
            ReferenceValue::I32(0),
            ReferenceValue::I32(ALIAS_PTR as i32),
            ReferenceValue::I32(ALIAS_LEN as i32),
            ReferenceValue::I32(STAT2 as i32),
        ],
    );
    let alias_after = reference_stat(memory, &store, STAT2);
    let mkdir_errno = reference_call(
        &instance,
        &mut store,
        "mkdir",
        &[
            ReferenceValue::I32(3),
            ReferenceValue::I32(DOCS_PTR as i32),
            ReferenceValue::I32(DOCS_LEN as i32),
        ],
    );
    let dir_stat_errno = reference_call(
        &instance,
        &mut store,
        "stat",
        &[
            ReferenceValue::I32(3),
            ReferenceValue::I32(0),
            ReferenceValue::I32(DOCS_PTR as i32),
            ReferenceValue::I32(DOCS_LEN as i32),
            ReferenceValue::I32(STAT3 as i32),
        ],
    );
    let directory_stat = reference_stat(memory, &store, STAT3);
    let symlink_errno = reference_call(
        &instance,
        &mut store,
        "symlink",
        &[
            ReferenceValue::I32(TARGET_PTR as i32),
            ReferenceValue::I32(TARGET_LEN as i32),
            ReferenceValue::I32(3),
            ReferenceValue::I32(LINK_PTR as i32),
            ReferenceValue::I32(LINK_LEN as i32),
        ],
    );
    let symlink_stat_errno = reference_call(
        &instance,
        &mut store,
        "stat",
        &[
            ReferenceValue::I32(3),
            ReferenceValue::I32(0),
            ReferenceValue::I32(LINK_PTR as i32),
            ReferenceValue::I32(LINK_LEN as i32),
            ReferenceValue::I32(STAT4 as i32),
        ],
    );
    let symlink_stat = reference_stat(memory, &store, STAT4);
    let open_dir_errno = reference_call(
        &instance,
        &mut store,
        "open",
        &[
            ReferenceValue::I32(3),
            ReferenceValue::I32(DOCS_PTR as i32),
            ReferenceValue::I32(DOCS_LEN as i32),
            ReferenceValue::I32(OFLAGS_DIRECTORY as i32),
            ReferenceValue::I64(DIRECTORY_RIGHTS as i64),
            ReferenceValue::I32(DIRECTORY_FD_OUT as i32),
        ],
    );
    let directory_fd = reference_u32(memory, &store, DIRECTORY_FD_OUT);
    let create_child_errno = reference_call(
        &instance,
        &mut store,
        "open",
        &[
            ReferenceValue::I32(directory_fd as i32),
            ReferenceValue::I32(NOTE_PTR as i32),
            ReferenceValue::I32(NOTE_LEN as i32),
            ReferenceValue::I32(OFLAGS_CREAT as i32),
            ReferenceValue::I64(0),
            ReferenceValue::I32(CHILD_FD_OUT as i32),
        ],
    );
    let child_stat_errno = reference_call(
        &instance,
        &mut store,
        "stat",
        &[
            ReferenceValue::I32(directory_fd as i32),
            ReferenceValue::I32(0),
            ReferenceValue::I32(NOTE_PTR as i32),
            ReferenceValue::I32(NOTE_LEN as i32),
            ReferenceValue::I32(STAT5 as i32),
        ],
    );
    let child_stat = reference_stat(memory, &store, STAT5);

    PathFilestatTrace {
        errnos: [
            stat_before_errno,
            link_errno,
            stat_after_errno,
            alias_stat_errno,
            mkdir_errno,
            dir_stat_errno,
            symlink_errno,
            symlink_stat_errno,
            open_dir_errno,
            create_child_errno,
            child_stat_errno,
        ],
        regular_before: regular_before.into(),
        regular_after_link: regular_after.into(),
        alias_after_link: alias_after.into(),
        original_inode_stable: regular_before.ino == regular_after.ino,
        linked_inode_equal: regular_after.ino == alias_after.ino,
        directory_filetype: directory_stat.filetype,
        symlink_filetype: symlink_stat.filetype,
        symlink_size: symlink_stat.size,
        child_filetype: child_stat.filetype,
        child_size: child_stat.size,
    }
}

fn expected_trace() -> PathFilestatTrace {
    PathFilestatTrace {
        errnos: [ERRNO_SUCCESS; 11],
        regular_before: PortableStat {
            filetype: FILETYPE_REGULAR_FILE,
            nlink: 1,
            size: 3,
        },
        regular_after_link: PortableStat {
            filetype: FILETYPE_REGULAR_FILE,
            nlink: 2,
            size: 3,
        },
        alias_after_link: PortableStat {
            filetype: FILETYPE_REGULAR_FILE,
            nlink: 2,
            size: 3,
        },
        original_inode_stable: true,
        linked_inode_equal: true,
        directory_filetype: FILETYPE_DIRECTORY,
        symlink_filetype: FILETYPE_SYMBOLIC_LINK,
        symlink_size: TARGET_LEN as u64,
        child_filetype: FILETYPE_REGULAR_FILE,
        child_size: 0,
    }
}

#[test]
fn path_filestat_get_matches_wasmtime_for_portable_nonfollowing_metadata() {
    let bytes = module_bytes();
    let expected = expected_trace();
    let mini = run_mini(&bytes);
    assert_eq!(mini, expected, "mini path-filestat trace");

    let engine = Engine::default();
    let reference = run_reference(&engine, &bytes);
    assert_eq!(reference, expected, "Wasmtime path-filestat trace");
    assert_eq!(mini, reference, "mini/Wasmtime path-filestat mismatch");
}
