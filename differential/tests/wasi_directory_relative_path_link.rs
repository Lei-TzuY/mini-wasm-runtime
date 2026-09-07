use std::{
    fs,
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
};

use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance as MiniInstance, MemoryHandle, Value};
use wasm_wasi::{
    WasiPreview1, ERRNO_SUCCESS, FILETYPE_REGULAR_FILE, OFLAGS_CREAT, OFLAGS_DIRECTORY,
    RIGHTS_FD_READ, RIGHTS_FD_READDIR, RIGHTS_PATH_CREATE_DIRECTORY, RIGHTS_PATH_CREATE_FILE,
    RIGHTS_PATH_LINK_SOURCE, RIGHTS_PATH_LINK_TARGET, RIGHTS_PATH_OPEN, RIGHTS_PATH_UNLINK_FILE,
};
use wasmtime::{Engine, Linker, Memory, MemoryType, Module as ReferenceModule, Store};
use wasmtime_wasi::{
    p1::{self, WasiP1Ctx},
    DirPerms, FilePerms, WasiCtxBuilder,
};

const LEFT_PTR: u32 = 1024;
const RIGHT_PTR: u32 = 1040;
const NOTE_PTR: u32 = 1056;
const ALIAS_PTR: u32 = 1072;
const LEFT_FD_OUT: u32 = 64;
const RIGHT_FD_OUT: u32 = 68;
const FILE_FD_OUT: u32 = 72;
const BUFUSED: u32 = 76;
const BUFFER: u32 = 128;
const BUFFER_LEN: u32 = 512;
const DIRENT_SIZE: usize = 24;
const DIRECTORY_RIGHTS: u64 = RIGHTS_FD_READDIR
    | RIGHTS_PATH_OPEN
    | RIGHTS_PATH_CREATE_DIRECTORY
    | RIGHTS_PATH_CREATE_FILE
    | RIGHTS_PATH_LINK_SOURCE
    | RIGHTS_PATH_LINK_TARGET
    | RIGHTS_PATH_UNLINK_FILE;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Dirent {
    ino: u64,
    filetype: u8,
    name: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PortableDirent {
    filetype: u8,
    name: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LinkTrace {
    errnos: [i32; 11],
    left_after_link: Vec<PortableDirent>,
    right_after_link: Vec<PortableDirent>,
    right_after_source_unlink: Vec<PortableDirent>,
    linked_inode_identity: bool,
    final_paths_absent: bool,
}

struct IsolatedDirectory {
    path: PathBuf,
}

impl IsolatedDirectory {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mini-wasm-runtime-wasi-directory-relative-link-{}-{id}",
            process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).expect("remove stale hard-link differential root");
        }
        fs::create_dir(&path).expect("create isolated hard-link differential root");
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
            (import "wasi_snapshot_preview1" "path_create_directory"
                (func $mkdir (param i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_open"
                (func $path_open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_link"
                (func $path_link (param i32 i32 i32 i32 i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_readdir"
                (func $fd_readdir (param i32 i32 i32 i64 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_unlink_file"
                (func $unlink (param i32 i32 i32) (result i32)))
            (import "env" "memory" (memory 1 1))
            (export "memory" (memory 0))

            (func (export "mkdir") (param $fd i32) (param $path i32) (param $len i32) (result i32)
                local.get $fd local.get $path local.get $len call $mkdir)
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
            (func (export "readdir")
                (param $fd i32) (param $buf i32) (param $len i32) (param $used i32) (result i32)
                local.get $fd local.get $buf local.get $len i64.const 0 local.get $used call $fd_readdir)
            (func (export "unlink") (param $fd i32) (param $path i32) (param $len i32) (result i32)
                local.get $fd local.get $path local.get $len call $unlink)
        )
        "#,
    )
    .expect("compile directory-relative path_link differential module")
}

fn parse_dirents(bytes: &[u8]) -> Vec<Dirent> {
    let mut cursor = 0;
    let mut entries = Vec::new();
    while bytes.len().saturating_sub(cursor) >= DIRENT_SIZE {
        let header = &bytes[cursor..cursor + DIRENT_SIZE];
        let name_len =
            u32::from_le_bytes(header[16..20].try_into().expect("dirent name length")) as usize;
        let start = cursor + DIRENT_SIZE;
        let Some(end) = start.checked_add(name_len) else {
            break;
        };
        if end > bytes.len() {
            break;
        }
        entries.push(Dirent {
            ino: u64::from_le_bytes(header[8..16].try_into().expect("dirent inode")),
            filetype: header[20],
            name: bytes[start..end].to_vec(),
        });
        cursor = end;
    }
    entries
}

fn portable(entries: &[Dirent]) -> Vec<PortableDirent> {
    let mut entries = entries
        .iter()
        .map(|entry| PortableDirent {
            filetype: entry.filetype,
            name: entry.name.clone(),
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    entries
}

fn linked_inode_identity(left: &[Dirent], right: &[Dirent]) -> bool {
    let source = left.iter().find(|entry| entry.name == b"note.txt");
    let alias = right.iter().find(|entry| entry.name == b"alias.txt");
    matches!((source, alias), (Some(source), Some(alias)) if source.filetype == FILETYPE_REGULAR_FILE && alias.filetype == FILETYPE_REGULAR_FILE && source.ino == alias.ino)
}

fn mini_call(instance: &mut MiniInstance, name: &str, args: &[Value]) -> i32 {
    match instance
        .invoke_export_values(name, args)
        .unwrap_or_else(|error| panic!("mini {name} trapped: {error:?}"))
        .as_slice()
    {
        [Value::I32(errno)] => *errno,
        other => panic!("mini {name} returned {other:?}"),
    }
}

fn mini_u32(memory: &MemoryHandle, address: u32) -> u32 {
    u32::from_le_bytes(memory.read(address, 4).unwrap().try_into().unwrap())
}

fn mini_readdir(instance: &mut MiniInstance, memory: &MemoryHandle, fd: u32) -> (i32, Vec<Dirent>) {
    memory.write(BUFFER, &[0x5a; BUFFER_LEN as usize]).unwrap();
    let errno = mini_call(
        instance,
        "readdir",
        &[
            Value::I32(fd as i32),
            Value::I32(BUFFER as i32),
            Value::I32(BUFFER_LEN as i32),
            Value::I32(BUFUSED as i32),
        ],
    );
    let used = mini_u32(memory, BUFUSED);
    (
        errno,
        parse_dirents(&memory.read(BUFFER, used as usize).unwrap()),
    )
}

fn run_mini(bytes: &[u8]) -> LinkTrace {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap();
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .unwrap();
    wasi.register(&mut hosts).unwrap();
    let module = parse_module(bytes).unwrap();
    let mut instance = MiniInstance::with_hosts(module, hosts).unwrap();

    memory.write(LEFT_PTR, b"left").unwrap();
    memory.write(RIGHT_PTR, b"right").unwrap();
    memory.write(NOTE_PTR, b"note.txt").unwrap();
    memory.write(ALIAS_PTR, b"alias.txt").unwrap();

    let mkdir_left = mini_call(
        &mut instance,
        "mkdir",
        &[Value::I32(3), Value::I32(LEFT_PTR as i32), Value::I32(4)],
    );
    let mkdir_right = mini_call(
        &mut instance,
        "mkdir",
        &[Value::I32(3), Value::I32(RIGHT_PTR as i32), Value::I32(5)],
    );
    let open_left = mini_call(
        &mut instance,
        "open",
        &[
            Value::I32(3),
            Value::I32(LEFT_PTR as i32),
            Value::I32(4),
            Value::I32(OFLAGS_DIRECTORY as i32),
            Value::I64(DIRECTORY_RIGHTS as i64),
            Value::I32(LEFT_FD_OUT as i32),
        ],
    );
    let open_right = mini_call(
        &mut instance,
        "open",
        &[
            Value::I32(3),
            Value::I32(RIGHT_PTR as i32),
            Value::I32(5),
            Value::I32(OFLAGS_DIRECTORY as i32),
            Value::I64(DIRECTORY_RIGHTS as i64),
            Value::I32(RIGHT_FD_OUT as i32),
        ],
    );
    let left_fd = mini_u32(&memory, LEFT_FD_OUT);
    let right_fd = mini_u32(&memory, RIGHT_FD_OUT);
    let create = mini_call(
        &mut instance,
        "open",
        &[
            Value::I32(left_fd as i32),
            Value::I32(NOTE_PTR as i32),
            Value::I32(8),
            Value::I32(OFLAGS_CREAT as i32),
            Value::I64(RIGHTS_FD_READ as i64),
            Value::I32(FILE_FD_OUT as i32),
        ],
    );
    let link = mini_call(
        &mut instance,
        "link",
        &[
            Value::I32(left_fd as i32),
            Value::I32(NOTE_PTR as i32),
            Value::I32(8),
            Value::I32(right_fd as i32),
            Value::I32(ALIAS_PTR as i32),
            Value::I32(9),
        ],
    );
    let (read_left, left_entries) = mini_readdir(&mut instance, &memory, left_fd);
    let (read_right, right_entries) = mini_readdir(&mut instance, &memory, right_fd);
    let unlink_source = mini_call(
        &mut instance,
        "unlink",
        &[
            Value::I32(left_fd as i32),
            Value::I32(NOTE_PTR as i32),
            Value::I32(8),
        ],
    );
    let (read_right_after, right_after_source_unlink) =
        mini_readdir(&mut instance, &memory, right_fd);
    let unlink_alias = mini_call(
        &mut instance,
        "unlink",
        &[
            Value::I32(right_fd as i32),
            Value::I32(ALIAS_PTR as i32),
            Value::I32(9),
        ],
    );

    LinkTrace {
        errnos: [
            mkdir_left,
            mkdir_right,
            open_left,
            open_right,
            create,
            link,
            read_left,
            read_right,
            unlink_source,
            read_right_after,
            unlink_alias,
        ],
        left_after_link: portable(&left_entries),
        right_after_link: portable(&right_entries),
        right_after_source_unlink: portable(&right_after_source_unlink),
        linked_inode_identity: linked_inode_identity(&left_entries, &right_entries),
        final_paths_absent: wasi.file_snapshot("/sandbox", "left/note.txt").is_none()
            && wasi.file_snapshot("/sandbox", "right/alias.txt").is_none(),
    }
}

fn reference_call(
    instance: &wasmtime::Instance,
    store: &mut Store<WasiP1Ctx>,
    name: &str,
    args: &[wasmtime::Val],
) -> i32 {
    let function = instance.get_func(&mut *store, name).unwrap();
    let mut results = [wasmtime::Val::I32(0)];
    function.call(&mut *store, args, &mut results).unwrap();
    match results[0] {
        wasmtime::Val::I32(errno) => errno,
        ref other => panic!("Wasmtime {name} returned {other:?}"),
    }
}

fn reference_u32(memory: Memory, store: &Store<WasiP1Ctx>, address: u32) -> u32 {
    let mut bytes = [0; 4];
    memory.read(store, address as usize, &mut bytes).unwrap();
    u32::from_le_bytes(bytes)
}

fn reference_readdir(
    instance: &wasmtime::Instance,
    store: &mut Store<WasiP1Ctx>,
    memory: Memory,
    fd: u32,
) -> (i32, Vec<Dirent>) {
    memory
        .write(&mut *store, BUFFER as usize, &[0x5a; BUFFER_LEN as usize])
        .unwrap();
    let errno = reference_call(
        instance,
        store,
        "readdir",
        &[
            wasmtime::Val::I32(fd as i32),
            wasmtime::Val::I32(BUFFER as i32),
            wasmtime::Val::I32(BUFFER_LEN as i32),
            wasmtime::Val::I32(BUFUSED as i32),
        ],
    );
    let used = reference_u32(memory, store, BUFUSED);
    let mut bytes = vec![0; used as usize];
    memory.read(store, BUFFER as usize, &mut bytes).unwrap();
    (errno, parse_dirents(&bytes))
}

fn run_reference(engine: &Engine, bytes: &[u8]) -> LinkTrace {
    let root = IsolatedDirectory::new();
    let module = ReferenceModule::new(engine, bytes).unwrap();
    let mut builder = WasiCtxBuilder::new();
    builder
        .preopened_dir(root.path(), "/sandbox", DirPerms::all(), FilePerms::all())
        .unwrap();
    let mut store = Store::new(engine, builder.build_p1());
    let memory = Memory::new(&mut store, MemoryType::new(1, Some(1))).unwrap();
    let mut linker: Linker<WasiP1Ctx> = Linker::new(engine);
    p1::add_to_linker_sync(&mut linker, |context| context).unwrap();
    linker.define(&store, "env", "memory", memory).unwrap();
    let instance = linker.instantiate(&mut store, &module).unwrap();

    memory
        .write(&mut store, LEFT_PTR as usize, b"left")
        .unwrap();
    memory
        .write(&mut store, RIGHT_PTR as usize, b"right")
        .unwrap();
    memory
        .write(&mut store, NOTE_PTR as usize, b"note.txt")
        .unwrap();
    memory
        .write(&mut store, ALIAS_PTR as usize, b"alias.txt")
        .unwrap();

    let mkdir_left = reference_call(
        &instance,
        &mut store,
        "mkdir",
        &[
            wasmtime::Val::I32(3),
            wasmtime::Val::I32(LEFT_PTR as i32),
            wasmtime::Val::I32(4),
        ],
    );
    let mkdir_right = reference_call(
        &instance,
        &mut store,
        "mkdir",
        &[
            wasmtime::Val::I32(3),
            wasmtime::Val::I32(RIGHT_PTR as i32),
            wasmtime::Val::I32(5),
        ],
    );
    let open_left = reference_call(
        &instance,
        &mut store,
        "open",
        &[
            wasmtime::Val::I32(3),
            wasmtime::Val::I32(LEFT_PTR as i32),
            wasmtime::Val::I32(4),
            wasmtime::Val::I32(OFLAGS_DIRECTORY as i32),
            wasmtime::Val::I64(DIRECTORY_RIGHTS as i64),
            wasmtime::Val::I32(LEFT_FD_OUT as i32),
        ],
    );
    let open_right = reference_call(
        &instance,
        &mut store,
        "open",
        &[
            wasmtime::Val::I32(3),
            wasmtime::Val::I32(RIGHT_PTR as i32),
            wasmtime::Val::I32(5),
            wasmtime::Val::I32(OFLAGS_DIRECTORY as i32),
            wasmtime::Val::I64(DIRECTORY_RIGHTS as i64),
            wasmtime::Val::I32(RIGHT_FD_OUT as i32),
        ],
    );
    let left_fd = reference_u32(memory, &store, LEFT_FD_OUT);
    let right_fd = reference_u32(memory, &store, RIGHT_FD_OUT);
    let create = reference_call(
        &instance,
        &mut store,
        "open",
        &[
            wasmtime::Val::I32(left_fd as i32),
            wasmtime::Val::I32(NOTE_PTR as i32),
            wasmtime::Val::I32(8),
            wasmtime::Val::I32(OFLAGS_CREAT as i32),
            wasmtime::Val::I64(RIGHTS_FD_READ as i64),
            wasmtime::Val::I32(FILE_FD_OUT as i32),
        ],
    );
    let link = reference_call(
        &instance,
        &mut store,
        "link",
        &[
            wasmtime::Val::I32(left_fd as i32),
            wasmtime::Val::I32(NOTE_PTR as i32),
            wasmtime::Val::I32(8),
            wasmtime::Val::I32(right_fd as i32),
            wasmtime::Val::I32(ALIAS_PTR as i32),
            wasmtime::Val::I32(9),
        ],
    );
    let (read_left, left_entries) = reference_readdir(&instance, &mut store, memory, left_fd);
    let (read_right, right_entries) = reference_readdir(&instance, &mut store, memory, right_fd);
    let unlink_source = reference_call(
        &instance,
        &mut store,
        "unlink",
        &[
            wasmtime::Val::I32(left_fd as i32),
            wasmtime::Val::I32(NOTE_PTR as i32),
            wasmtime::Val::I32(8),
        ],
    );
    let (read_right_after, right_after_source_unlink) =
        reference_readdir(&instance, &mut store, memory, right_fd);
    let unlink_alias = reference_call(
        &instance,
        &mut store,
        "unlink",
        &[
            wasmtime::Val::I32(right_fd as i32),
            wasmtime::Val::I32(ALIAS_PTR as i32),
            wasmtime::Val::I32(9),
        ],
    );

    LinkTrace {
        errnos: [
            mkdir_left,
            mkdir_right,
            open_left,
            open_right,
            create,
            link,
            read_left,
            read_right,
            unlink_source,
            read_right_after,
            unlink_alias,
        ],
        left_after_link: portable(&left_entries),
        right_after_link: portable(&right_entries),
        right_after_source_unlink: portable(&right_after_source_unlink),
        linked_inode_identity: linked_inode_identity(&left_entries, &right_entries),
        final_paths_absent: !root.path().join("left/note.txt").exists()
            && !root.path().join("right/alias.txt").exists(),
    }
}

#[test]
fn directory_relative_path_link_matches_wasmtime() {
    let bytes = module_bytes();
    let mini = run_mini(&bytes);
    let reference = run_reference(&Engine::default(), &bytes);

    assert_eq!(mini.errnos, [ERRNO_SUCCESS; 11]);
    assert!(mini.linked_inode_identity);
    assert!(reference.linked_inode_identity);
    assert!(mini.final_paths_absent);
    assert!(reference.final_paths_absent);
    assert_eq!(mini, reference);
}
