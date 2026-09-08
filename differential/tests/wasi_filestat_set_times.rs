use std::{
    fs,
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
};

use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance as MiniInstance, MemoryHandle, Value};
use wasm_wasi::{
    WasiPreview1, ERRNO_SUCCESS, RIGHTS_FD_FILESTAT_GET, RIGHTS_FD_FILESTAT_SET_TIMES,
};
use wasmtime::{Engine, Linker, Memory, MemoryType, Module as ReferenceModule, Store, Val};
use wasmtime_wasi::{
    p1::{self, WasiP1Ctx},
    DirPerms, FilePerms, WasiCtxBuilder,
};

const PATH_PTR: u32 = 64;
const PATH_LEN: u32 = 8;
const FD_OUT: u32 = 96;
const FD_STAT: u32 = 128;
const PATH_STAT: u32 = 192;
const ATIM: u64 = 1_700_000_000_123_456_789;
const MTIM: u64 = 1_700_000_000_987_654_321;
const FSTFLAGS_ATIM: i32 = 1;
const FSTFLAGS_MTIM: i32 = 4;
const RIGHTS: u64 = RIGHTS_FD_FILESTAT_GET | RIGHTS_FD_FILESTAT_SET_TIMES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TimestampTrace {
    errnos: [i32; 6],
    fd_after_fd_set: (u64, u64),
    path_after_fd_set: (u64, u64),
    fd_after_path_set: (u64, u64),
}

struct IsolatedDirectory {
    path: PathBuf,
}

impl IsolatedDirectory {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mini-wasm-runtime-wasi-filestat-set-times-{}-{id}",
            process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).expect("remove stale timestamp differential root");
        }
        fs::create_dir(&path).expect("create timestamp differential root");
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
                (func $path_open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_filestat_get"
                (func $fd_filestat_get (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_filestat_set_times"
                (func $fd_filestat_set_times (param i32 i64 i64 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_filestat_get"
                (func $path_filestat_get (param i32 i32 i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_filestat_set_times"
                (func $path_filestat_set_times (param i32 i32 i32 i32 i64 i64 i32) (result i32)))
            (import "env" "memory" (memory 1 1))
            (export "memory" (memory 0))
            (data (i32.const 64) "data.bin")

            (func (export "open") (param $rights i64) (result i32)
                i32.const 3
                i32.const 0
                i32.const 64
                i32.const 8
                i32.const 0
                local.get $rights
                i64.const 0
                i32.const 0
                i32.const 96
                call $path_open)
            (func (export "fd_set") (param $fd i32) (param $atim i64) (param $mtim i64) (result i32)
                local.get $fd
                local.get $atim
                local.get $mtim
                i32.const 5
                call $fd_filestat_set_times)
            (func (export "fd_get") (param $fd i32) (param $out i32) (result i32)
                local.get $fd local.get $out call $fd_filestat_get)
            (func (export "path_set") (param $atim i64) (param $mtim i64) (result i32)
                i32.const 3
                i32.const 0
                i32.const 64
                i32.const 8
                local.get $atim
                local.get $mtim
                i32.const 5
                call $path_filestat_set_times)
            (func (export "path_get") (param $out i32) (result i32)
                i32.const 3
                i32.const 0
                i32.const 64
                i32.const 8
                local.get $out
                call $path_filestat_get)
        )
        "#,
    )
    .expect("compile deterministic WASI filestat timestamp module")
}

fn decode_times(bytes: &[u8]) -> (u64, u64) {
    assert_eq!(bytes.len(), 64, "filestat ABI width");
    (
        u64::from_le_bytes(bytes[40..48].try_into().expect("atim bytes")),
        u64::from_le_bytes(bytes[48..56].try_into().expect("mtim bytes")),
    )
}

fn mini_call(instance: &mut MiniInstance, export: &str, args: &[Value]) -> i32 {
    match instance
        .invoke_export_values(export, args)
        .unwrap_or_else(|error| panic!("mini timestamp call {export:?} trapped: {error:?}"))
        .as_slice()
    {
        [Value::I32(errno)] => *errno,
        other => panic!("mini timestamp call {export:?} returned {other:?}"),
    }
}

fn mini_u32(memory: &MemoryHandle, ptr: u32) -> u32 {
    u32::from_le_bytes(
        memory
            .read(ptr, 4)
            .expect("read mini fd")
            .try_into()
            .expect("fixed u32 width"),
    )
}

fn mini_times(memory: &MemoryHandle, ptr: u32) -> (u64, u64) {
    decode_times(&memory.read(ptr, 64).expect("read mini filestat"))
}

fn run_mini(bytes: &[u8]) -> TimestampTrace {
    let memory = MemoryHandle::new(1, Some(1)).expect("create mini timestamp memory");
    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .expect("configure mini writable preopen")
        .with_writable_file("/sandbox", "data.bin", b"abc")
        .expect("mount mini timestamp file");
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .expect("register mini timestamp memory");
    wasi.register(&mut hosts)
        .expect("register mini timestamp WASI surface");
    let module = parse_module(bytes).expect("timestamp module parses in mini runtime");
    let mut instance = MiniInstance::with_hosts(module, hosts)
        .expect("timestamp module instantiates in mini runtime");

    let open = mini_call(&mut instance, "open", &[Value::I64(RIGHTS as i64)]);
    let fd = mini_u32(&memory, FD_OUT);
    let fd_set = mini_call(
        &mut instance,
        "fd_set",
        &[Value::I32(fd as i32), Value::I64(ATIM as i64), Value::I64(MTIM as i64)],
    );
    let fd_get = mini_call(
        &mut instance,
        "fd_get",
        &[Value::I32(fd as i32), Value::I32(FD_STAT as i32)],
    );
    let fd_after_fd_set = mini_times(&memory, FD_STAT);
    let path_get = mini_call(
        &mut instance,
        "path_get",
        &[Value::I32(PATH_STAT as i32)],
    );
    let path_after_fd_set = mini_times(&memory, PATH_STAT);
    let path_set = mini_call(
        &mut instance,
        "path_set",
        &[Value::I64((ATIM + 11) as i64), Value::I64((MTIM + 22) as i64)],
    );
    let fd_get_after = mini_call(
        &mut instance,
        "fd_get",
        &[Value::I32(fd as i32), Value::I32(FD_STAT as i32)],
    );
    let fd_after_path_set = mini_times(&memory, FD_STAT);

    TimestampTrace {
        errnos: [open, fd_set, fd_get, path_get, path_set, fd_get_after],
        fd_after_fd_set,
        path_after_fd_set,
        fd_after_path_set,
    }
}

fn reference_call(
    instance: &wasmtime::Instance,
    store: &mut Store<WasiP1Ctx>,
    export: &str,
    args: &[Val],
) -> i32 {
    let function = instance
        .get_func(&mut *store, export)
        .unwrap_or_else(|| panic!("resolve Wasmtime timestamp export {export:?}"));
    let mut results = [Val::I32(0)];
    function
        .call(&mut *store, args, &mut results)
        .unwrap_or_else(|error| panic!("Wasmtime timestamp call {export:?} trapped: {error}"));
    match results[0] {
        Val::I32(errno) => errno,
        ref other => panic!("Wasmtime timestamp call {export:?} returned {other:?}"),
    }
}

fn reference_u32(memory: Memory, store: &Store<WasiP1Ctx>, ptr: u32) -> u32 {
    let mut bytes = [0_u8; 4];
    memory
        .read(store, ptr as usize, &mut bytes)
        .expect("read Wasmtime fd");
    u32::from_le_bytes(bytes)
}

fn reference_times(memory: Memory, store: &Store<WasiP1Ctx>, ptr: u32) -> (u64, u64) {
    let mut bytes = [0_u8; 64];
    memory
        .read(store, ptr as usize, &mut bytes)
        .expect("read Wasmtime filestat");
    decode_times(&bytes)
}

fn run_reference(engine: &Engine, bytes: &[u8]) -> TimestampTrace {
    let root = IsolatedDirectory::new();
    fs::write(root.path().join("data.bin"), b"abc").expect("seed Wasmtime timestamp file");
    let module = ReferenceModule::new(engine, bytes).expect("compile Wasmtime timestamp module");
    let mut builder = WasiCtxBuilder::new();
    builder
        .preopened_dir(root.path(), "/sandbox", DirPerms::all(), FilePerms::all())
        .expect("configure Wasmtime timestamp preopen");
    let mut store = Store::new(engine, builder.build_p1());
    let memory = Memory::new(&mut store, MemoryType::new(1, Some(1)))
        .expect("create Wasmtime timestamp memory");
    let mut linker: Linker<WasiP1Ctx> = Linker::new(engine);
    p1::add_to_linker_sync(&mut linker, |context| context)
        .expect("register Wasmtime timestamp WASI imports");
    linker
        .define(&store, "env", "memory", memory)
        .expect("register Wasmtime timestamp memory");
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiate timestamp module in Wasmtime");

    let open = reference_call(&instance, &mut store, "open", &[Val::I64(RIGHTS as i64)]);
    let fd = reference_u32(memory, &store, FD_OUT);
    let fd_set = reference_call(
        &instance,
        &mut store,
        "fd_set",
        &[Val::I32(fd as i32), Val::I64(ATIM as i64), Val::I64(MTIM as i64)],
    );
    let fd_get = reference_call(
        &instance,
        &mut store,
        "fd_get",
        &[Val::I32(fd as i32), Val::I32(FD_STAT as i32)],
    );
    let fd_after_fd_set = reference_times(memory, &store, FD_STAT);
    let path_get = reference_call(
        &instance,
        &mut store,
        "path_get",
        &[Val::I32(PATH_STAT as i32)],
    );
    let path_after_fd_set = reference_times(memory, &store, PATH_STAT);
    let path_set = reference_call(
        &instance,
        &mut store,
        "path_set",
        &[Val::I64((ATIM + 11) as i64), Val::I64((MTIM + 22) as i64)],
    );
    let fd_get_after = reference_call(
        &instance,
        &mut store,
        "fd_get",
        &[Val::I32(fd as i32), Val::I32(FD_STAT as i32)],
    );
    let fd_after_path_set = reference_times(memory, &store, FD_STAT);

    TimestampTrace {
        errnos: [open, fd_set, fd_get, path_get, path_set, fd_get_after],
        fd_after_fd_set,
        path_after_fd_set,
        fd_after_path_set,
    }
}

#[test]
fn explicit_filestat_timestamp_mutation_matches_wasmtime_wasi_37_0_3() {
    let bytes = module_bytes();
    let mini = run_mini(&bytes);
    let reference = run_reference(&Engine::default(), &bytes);
    let expected = TimestampTrace {
        errnos: [ERRNO_SUCCESS; 6],
        fd_after_fd_set: (ATIM, MTIM),
        path_after_fd_set: (ATIM, MTIM),
        fd_after_path_set: (ATIM + 11, MTIM + 22),
    };

    assert_eq!(mini, expected, "mini timestamp mutation must match contract");
    assert_eq!(
        reference, expected,
        "Wasmtime-WASI 37.0.3 timestamp mutation must match portable contract"
    );
    assert_eq!(mini, reference, "mini and Wasmtime timestamp traces diverged");
}
