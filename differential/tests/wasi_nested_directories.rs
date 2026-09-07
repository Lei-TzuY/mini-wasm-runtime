use std::{
    fs,
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
};

use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance as MiniInstance, MemoryHandle, Value};
use wasm_wasi::{
    WasiPreview1, ERRNO_NOTEMPTY, ERRNO_SUCCESS, FILETYPE_DIRECTORY, FILETYPE_REGULAR_FILE,
    OFLAGS_CREAT, OFLAGS_DIRECTORY, RIGHTS_FD_READDIR, RIGHTS_PATH_CREATE_FILE, RIGHTS_PATH_OPEN,
};
use wasmtime::{Engine, Linker, Memory, MemoryType, Module as ReferenceModule, Store};
use wasmtime_wasi::{
    p1::{self, WasiP1Ctx},
    DirPerms, FilePerms, WasiCtxBuilder,
};

const DOCS_PTR: u32 = 1024;
const NOTE_PTR: u32 = 1040;
const FULL_NOTE_PTR: u32 = 1060;
const ROOT_FD_OUT: u32 = 64;
const FILE_FD_OUT: u32 = 68;
const BUFUSED: u32 = 72;
const BUFFER: u32 = 128;
const BUFFER_LEN: u32 = 512;
const DIRENT_SIZE: usize = 24;
const DIRECTORY_RIGHTS: u64 = RIGHTS_FD_READDIR | RIGHTS_PATH_OPEN | RIGHTS_PATH_CREATE_FILE;

#[derive(Debug, Clone, PartialEq, Eq)]
struct PortableDirent {
    filetype: u8,
    name: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NestedDirectoryTrace {
    errnos: [i32; 9],
    root_after_create: Vec<PortableDirent>,
    nested_after_file_create: Vec<PortableDirent>,
    root_after_remove: Vec<PortableDirent>,
    reference_directory_removed: bool,
}

struct IsolatedDirectory {
    path: PathBuf,
}

impl IsolatedDirectory {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mini-wasm-runtime-wasi-nested-directories-{}-{id}",
            process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).expect("remove stale nested-directory differential root");
        }
        fs::create_dir(&path).expect("create isolated nested-directory differential root");
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
            (import "wasi_snapshot_preview1" "path_remove_directory"
                (func $rmdir (param i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_open"
                (func $path_open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_readdir"
                (func $fd_readdir (param i32 i32 i32 i64 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_unlink_file"
                (func $unlink (param i32 i32 i32) (result i32)))
            (import "env" "memory" (memory 1 1))
            (export "memory" (memory 0))

            (func (export "mkdir") (param $fd i32) (param $path i32) (param $len i32) (result i32)
                local.get $fd local.get $path local.get $len call $mkdir)
            (func (export "rmdir") (param $fd i32) (param $path i32) (param $len i32) (result i32)
                local.get $fd local.get $path local.get $len call $rmdir)
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
            (func (export "readdir")
                (param $fd i32) (param $buf i32) (param $len i32) (param $used i32) (result i32)
                local.get $fd local.get $buf local.get $len i64.const 0 local.get $used call $fd_readdir)
            (func (export "unlink") (param $fd i32) (param $path i32) (param $len i32) (result i32)
                local.get $fd local.get $path local.get $len call $unlink)
        )
        "#,
    )
    .expect("compile nested-directory differential module")
}

fn parse_dirents(bytes: &[u8]) -> Vec<PortableDirent> {
    let mut cursor = 0;
    let mut entries = Vec::new();
    while bytes.len().saturating_sub(cursor) >= DIRENT_SIZE {
        let header = &bytes[cursor..cursor + DIRENT_SIZE];
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
        entries.push(PortableDirent {
            filetype,
            name: bytes[name_start..name_end].to_vec(),
        });
        cursor = name_end;
    }
    entries
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
    u32::from_le_bytes(
        memory
            .read(address, 4)
            .expect("read mini u32")
            .try_into()
            .expect("fixed u32 width"),
    )
}

fn mini_readdir(
    instance: &mut MiniInstance,
    memory: &MemoryHandle,
    fd: u32,
) -> (i32, Vec<PortableDirent>) {
    memory
        .write(BUFFER, &[0x5a; BUFFER_LEN as usize])
        .expect("seed mini readdir buffer");
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
    let bytes = memory
        .read(BUFFER, used as usize)
        .expect("read mini readdir payload");
    (errno, parse_dirents(&bytes))
}

fn run_mini(bytes: &[u8]) -> NestedDirectoryTrace {
    let memory = MemoryHandle::new(1, Some(1)).expect("create mini nested-directory memory");
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .expect("configure mini writable preopen");
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .expect("register mini nested-directory memory");
    wasi.register(&mut hosts)
        .expect("register mini WASI surface");
    let module = parse_module(bytes).expect("nested-directory module parses in mini runtime");
    let mut instance = MiniInstance::with_hosts(module, hosts)
        .expect("nested-directory module instantiates in mini runtime");

    memory.write(DOCS_PTR, b"docs").expect("write docs path");
    memory
        .write(NOTE_PTR, b"note.txt")
        .expect("write note path");
    memory
        .write(FULL_NOTE_PTR, b"docs/note.txt")
        .expect("write full note path");

    let mkdir_errno = mini_call(
        &mut instance,
        "mkdir",
        &[Value::I32(3), Value::I32(DOCS_PTR as i32), Value::I32(4)],
    );
    let (root_readdir_errno, root_after_create) = mini_readdir(&mut instance, &memory, 3);
    let open_dir_errno = mini_call(
        &mut instance,
        "open",
        &[
            Value::I32(3),
            Value::I32(DOCS_PTR as i32),
            Value::I32(4),
            Value::I32(OFLAGS_DIRECTORY as i32),
            Value::I64(DIRECTORY_RIGHTS as i64),
            Value::I32(ROOT_FD_OUT as i32),
        ],
    );
    let directory_fd = mini_u32(&memory, ROOT_FD_OUT);
    let create_file_errno = mini_call(
        &mut instance,
        "open",
        &[
            Value::I32(directory_fd as i32),
            Value::I32(NOTE_PTR as i32),
            Value::I32(8),
            Value::I32(OFLAGS_CREAT as i32),
            Value::I64(0),
            Value::I32(FILE_FD_OUT as i32),
        ],
    );
    let (nested_readdir_errno, nested_after_file_create) =
        mini_readdir(&mut instance, &memory, directory_fd);
    let nonempty_rmdir_errno = mini_call(
        &mut instance,
        "rmdir",
        &[Value::I32(3), Value::I32(DOCS_PTR as i32), Value::I32(4)],
    );
    let unlink_errno = mini_call(
        &mut instance,
        "unlink",
        &[
            Value::I32(3),
            Value::I32(FULL_NOTE_PTR as i32),
            Value::I32(13),
        ],
    );
    let remove_errno = mini_call(
        &mut instance,
        "rmdir",
        &[Value::I32(3), Value::I32(DOCS_PTR as i32), Value::I32(4)],
    );
    let (final_readdir_errno, root_after_remove) = mini_readdir(&mut instance, &memory, 3);

    NestedDirectoryTrace {
        errnos: [
            mkdir_errno,
            root_readdir_errno,
            open_dir_errno,
            create_file_errno,
            nested_readdir_errno,
            nonempty_rmdir_errno,
            unlink_errno,
            remove_errno,
            final_readdir_errno,
        ],
        root_after_create,
        nested_after_file_create,
        root_after_remove,
        reference_directory_removed: true,
    }
}

fn reference_call(
    instance: &wasmtime::Instance,
    store: &mut Store<WasiP1Ctx>,
    name: &str,
    args: &[wasmtime::Val],
) -> i32 {
    let function = instance
        .get_func(&mut *store, name)
        .unwrap_or_else(|| panic!("resolve Wasmtime {name} export"));
    let mut results = [wasmtime::Val::I32(0)];
    function
        .call(&mut *store, args, &mut results)
        .unwrap_or_else(|error| panic!("Wasmtime {name} trapped: {error}"));
    match results[0] {
        wasmtime::Val::I32(errno) => errno,
        ref other => panic!("Wasmtime {name} returned {other:?}"),
    }
}

fn reference_u32(memory: Memory, store: &Store<WasiP1Ctx>, address: u32) -> u32 {
    let mut bytes = [0_u8; 4];
    memory
        .read(store, address as usize, &mut bytes)
        .expect("read Wasmtime u32");
    u32::from_le_bytes(bytes)
}

fn reference_readdir(
    instance: &wasmtime::Instance,
    store: &mut Store<WasiP1Ctx>,
    memory: Memory,
    fd: u32,
) -> (i32, Vec<PortableDirent>) {
    memory
        .write(&mut *store, BUFFER as usize, &[0x5a; BUFFER_LEN as usize])
        .expect("seed Wasmtime readdir buffer");
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
    memory
        .read(store, BUFFER as usize, &mut bytes)
        .expect("read Wasmtime readdir payload");
    (errno, parse_dirents(&bytes))
}

fn run_reference(engine: &Engine, bytes: &[u8]) -> NestedDirectoryTrace {
    let root = IsolatedDirectory::new();
    let module =
        ReferenceModule::new(engine, bytes).expect("compile nested-directory reference module");
    let mut builder = WasiCtxBuilder::new();
    builder
        .preopened_dir(root.path(), "/sandbox", DirPerms::all(), FilePerms::all())
        .expect("configure isolated writable Wasmtime preopen");
    let mut store = Store::new(engine, builder.build_p1());
    let memory = Memory::new(&mut store, MemoryType::new(1, Some(1)))
        .expect("create Wasmtime nested-directory memory");
    let mut linker: Linker<WasiP1Ctx> = Linker::new(engine);
    p1::add_to_linker_sync(&mut linker, |context| context)
        .expect("register Wasmtime WASI Preview1 nested-directory imports");
    linker
        .define(&store, "env", "memory", memory)
        .expect("register Wasmtime nested-directory memory");
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiate nested-directory module in Wasmtime");

    memory
        .write(&mut store, DOCS_PTR as usize, b"docs")
        .expect("write docs path");
    memory
        .write(&mut store, NOTE_PTR as usize, b"note.txt")
        .expect("write note path");
    memory
        .write(&mut store, FULL_NOTE_PTR as usize, b"docs/note.txt")
        .expect("write full note path");

    let mkdir_errno = reference_call(
        &instance,
        &mut store,
        "mkdir",
        &[
            wasmtime::Val::I32(3),
            wasmtime::Val::I32(DOCS_PTR as i32),
            wasmtime::Val::I32(4),
        ],
    );
    let (root_readdir_errno, root_after_create) =
        reference_readdir(&instance, &mut store, memory, 3);
    let open_dir_errno = reference_call(
        &instance,
        &mut store,
        "open",
        &[
            wasmtime::Val::I32(3),
            wasmtime::Val::I32(DOCS_PTR as i32),
            wasmtime::Val::I32(4),
            wasmtime::Val::I32(OFLAGS_DIRECTORY as i32),
            wasmtime::Val::I64(DIRECTORY_RIGHTS as i64),
            wasmtime::Val::I32(ROOT_FD_OUT as i32),
        ],
    );
    let directory_fd = reference_u32(memory, &store, ROOT_FD_OUT);
    let create_file_errno = reference_call(
        &instance,
        &mut store,
        "open",
        &[
            wasmtime::Val::I32(directory_fd as i32),
            wasmtime::Val::I32(NOTE_PTR as i32),
            wasmtime::Val::I32(8),
            wasmtime::Val::I32(OFLAGS_CREAT as i32),
            wasmtime::Val::I64(0),
            wasmtime::Val::I32(FILE_FD_OUT as i32),
        ],
    );
    let (nested_readdir_errno, nested_after_file_create) =
        reference_readdir(&instance, &mut store, memory, directory_fd);
    let nonempty_rmdir_errno = reference_call(
        &instance,
        &mut store,
        "rmdir",
        &[
            wasmtime::Val::I32(3),
            wasmtime::Val::I32(DOCS_PTR as i32),
            wasmtime::Val::I32(4),
        ],
    );
    let unlink_errno = reference_call(
        &instance,
        &mut store,
        "unlink",
        &[
            wasmtime::Val::I32(3),
            wasmtime::Val::I32(FULL_NOTE_PTR as i32),
            wasmtime::Val::I32(13),
        ],
    );
    let remove_errno = reference_call(
        &instance,
        &mut store,
        "rmdir",
        &[
            wasmtime::Val::I32(3),
            wasmtime::Val::I32(DOCS_PTR as i32),
            wasmtime::Val::I32(4),
        ],
    );
    let (final_readdir_errno, root_after_remove) =
        reference_readdir(&instance, &mut store, memory, 3);
    let reference_directory_removed = !root.path().join("docs").exists();

    NestedDirectoryTrace {
        errnos: [
            mkdir_errno,
            root_readdir_errno,
            open_dir_errno,
            create_file_errno,
            nested_readdir_errno,
            nonempty_rmdir_errno,
            unlink_errno,
            remove_errno,
            final_readdir_errno,
        ],
        root_after_create,
        nested_after_file_create,
        root_after_remove,
        reference_directory_removed,
    }
}

#[test]
fn nested_directory_lifecycle_matches_wasmtime_preview1() {
    let bytes = module_bytes();
    let mini = run_mini(&bytes);
    let reference = run_reference(&Engine::default(), &bytes);

    let expected = NestedDirectoryTrace {
        errnos: [
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_NOTEMPTY,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
            ERRNO_SUCCESS,
        ],
        root_after_create: vec![
            PortableDirent {
                filetype: FILETYPE_DIRECTORY,
                name: b".".to_vec(),
            },
            PortableDirent {
                filetype: FILETYPE_DIRECTORY,
                name: b"..".to_vec(),
            },
            PortableDirent {
                filetype: FILETYPE_DIRECTORY,
                name: b"docs".to_vec(),
            },
        ],
        nested_after_file_create: vec![
            PortableDirent {
                filetype: FILETYPE_DIRECTORY,
                name: b".".to_vec(),
            },
            PortableDirent {
                filetype: FILETYPE_DIRECTORY,
                name: b"..".to_vec(),
            },
            PortableDirent {
                filetype: FILETYPE_REGULAR_FILE,
                name: b"note.txt".to_vec(),
            },
        ],
        root_after_remove: vec![
            PortableDirent {
                filetype: FILETYPE_DIRECTORY,
                name: b".".to_vec(),
            },
            PortableDirent {
                filetype: FILETYPE_DIRECTORY,
                name: b"..".to_vec(),
            },
        ],
        reference_directory_removed: true,
    };

    assert_eq!(mini, expected, "mini nested-directory trace drifted");
    assert_eq!(
        reference, expected,
        "Wasmtime nested-directory trace drifted"
    );
}
