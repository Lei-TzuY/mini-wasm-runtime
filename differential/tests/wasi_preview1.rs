use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance as MiniInstance, MemoryHandle, Value};
use wasm_wasi::{WasiPreview1, ERRNO_SUCCESS};
use wasmtime::{Engine, Linker, Memory, MemoryType, Module as ReferenceModule, Store};
use wasmtime_wasi::{
    p1::{self, WasiP1Ctx},
    WasiCtxBuilder,
};

const SNAPSHOT_BYTES: usize = 192;
const ARGS_COUNT_PTR: usize = 0;
const ARGS_SIZE_PTR: usize = 4;
const ENV_COUNT_PTR: usize = 8;
const ENV_SIZE_PTR: usize = 12;
const ARGS_PTRS: usize = 16;
const ENV_PTRS: usize = 32;
const ARGS_PAYLOAD: usize = 64;
const ENV_PAYLOAD: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Snapshot {
    errnos: [i32; 4],
    memory: Vec<u8>,
}

fn module_bytes() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (import "wasi_snapshot_preview1" "args_sizes_get"
                (func $args_sizes_get (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "args_get"
                (func $args_get (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "environ_sizes_get"
                (func $environ_sizes_get (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "environ_get"
                (func $environ_get (param i32 i32) (result i32)))
            (import "env" "memory" (memory 1 1))
            (export "memory" (memory 0))

            (func (export "args_sizes") (result i32)
                i32.const 0
                i32.const 4
                call $args_sizes_get)
            (func (export "args_get") (result i32)
                i32.const 16
                i32.const 64
                call $args_get)
            (func (export "env_sizes") (result i32)
                i32.const 8
                i32.const 12
                call $environ_sizes_get)
            (func (export "env_get") (result i32)
                i32.const 32
                i32.const 128
                call $environ_get))
        "#,
    )
    .expect("compile deterministic WASI Preview1 interop module")
}

fn mini_errno(instance: &mut MiniInstance, export: &str) -> i32 {
    match instance
        .invoke_export(export, &[])
        .unwrap_or_else(|error| panic!("mini WASI interop call {export:?} trapped: {error:?}"))
    {
        Some(Value::I32(errno)) => errno,
        other => panic!("mini WASI interop call {export:?} returned {other:?}"),
    }
}

fn run_mini(bytes: &[u8], args: &[&str], env: &[(&str, &str)]) -> Snapshot {
    let memory = MemoryHandle::new(1, Some(1)).expect("create mini WASI interop memory");
    let wasi = WasiPreview1::new()
        .with_args(args.iter().copied())
        .with_env(env.iter().copied());
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .expect("register mini WASI interop memory");
    wasi.register(&mut hosts)
        .expect("register mini WASI Preview1 host surface");
    let module = parse_module(bytes).expect("WASI interop module must parse in mini runtime");
    let mut instance = MiniInstance::with_hosts(module, hosts)
        .expect("WASI interop module must instantiate in mini runtime");

    let errnos = [
        mini_errno(&mut instance, "args_sizes"),
        mini_errno(&mut instance, "args_get"),
        mini_errno(&mut instance, "env_sizes"),
        mini_errno(&mut instance, "env_get"),
    ];
    Snapshot {
        errnos,
        memory: memory
            .read(0, SNAPSHOT_BYTES)
            .expect("read mini WASI interop memory snapshot"),
    }
}

fn reference_errno(
    instance: &wasmtime::Instance,
    store: &mut Store<WasiP1Ctx>,
    export: &str,
) -> i32 {
    instance
        .get_typed_func::<(), i32>(&mut *store, export)
        .unwrap_or_else(|error| panic!("resolve Wasmtime WASI export {export:?}: {error}"))
        .call(&mut *store, ())
        .unwrap_or_else(|error| panic!("Wasmtime WASI interop call {export:?} trapped: {error}"))
}

fn run_reference(engine: &Engine, bytes: &[u8], args: &[&str], env: &[(&str, &str)]) -> Snapshot {
    let module =
        ReferenceModule::new(engine, bytes).expect("compile WASI interop module in Wasmtime");
    let mut builder = WasiCtxBuilder::new();
    builder.args(args).envs(env);
    let mut store = Store::new(engine, builder.build_p1());
    let memory = Memory::new(&mut store, MemoryType::new(1, Some(1)))
        .expect("create Wasmtime WASI interop memory");
    let mut linker: Linker<WasiP1Ctx> = Linker::new(engine);
    p1::add_to_linker_sync(&mut linker, |context| context)
        .expect("register Wasmtime WASI Preview1 imports");
    linker
        .define(&store, "env", "memory", memory)
        .expect("register Wasmtime WASI interop memory");
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiate WASI interop module in Wasmtime");

    let errnos = [
        reference_errno(&instance, &mut store, "args_sizes"),
        reference_errno(&instance, &mut store, "args_get"),
        reference_errno(&instance, &mut store, "env_sizes"),
        reference_errno(&instance, &mut store, "env_get"),
    ];
    let mut snapshot = vec![0_u8; SNAPSHOT_BYTES];
    memory
        .read(&store, 0, &mut snapshot)
        .expect("read Wasmtime WASI interop memory snapshot");
    Snapshot {
        errnos,
        memory: snapshot,
    }
}

fn write_u32(memory: &mut [u8], address: usize, value: u32) {
    memory[address..address + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_string_vector(memory: &mut [u8], pointers: usize, payload: usize, values: &[String]) {
    let mut cursor = payload;
    for (index, value) in values.iter().enumerate() {
        write_u32(memory, pointers + index * 4, cursor as u32);
        let bytes = value.as_bytes();
        memory[cursor..cursor + bytes.len()].copy_from_slice(bytes);
        cursor += bytes.len();
        memory[cursor] = 0;
        cursor += 1;
    }
}

fn expected_snapshot(args: &[&str], env: &[(&str, &str)]) -> Snapshot {
    let mut memory = vec![0_u8; SNAPSHOT_BYTES];
    let arg_values: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
    let env_values: Vec<String> = env
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect();
    let args_bytes = arg_values.iter().map(|arg| arg.len() + 1).sum::<usize>();
    let env_bytes = env_values
        .iter()
        .map(|entry| entry.len() + 1)
        .sum::<usize>();

    write_u32(&mut memory, ARGS_COUNT_PTR, arg_values.len() as u32);
    write_u32(&mut memory, ARGS_SIZE_PTR, args_bytes as u32);
    write_u32(&mut memory, ENV_COUNT_PTR, env_values.len() as u32);
    write_u32(&mut memory, ENV_SIZE_PTR, env_bytes as u32);
    write_string_vector(&mut memory, ARGS_PTRS, ARGS_PAYLOAD, &arg_values);
    write_string_vector(&mut memory, ENV_PTRS, ENV_PAYLOAD, &env_values);

    Snapshot {
        errnos: [ERRNO_SUCCESS; 4],
        memory,
    }
}

fn assert_case_matches_reference(args: &[&str], env: &[(&str, &str)]) {
    let bytes = module_bytes();
    let mini = run_mini(&bytes, args, env);
    let engine = Engine::default();
    let reference = run_reference(&engine, &bytes, args, env);
    let expected = expected_snapshot(args, env);

    assert_eq!(mini, expected, "mini WASI Preview1 layout mismatch");
    assert_eq!(
        reference, expected,
        "Wasmtime WASI Preview1 layout mismatch"
    );
    assert_eq!(mini, reference, "WASI Preview1 differential mismatch");
}

#[test]
fn deterministic_args_and_environment_match_wasmtime_wasi() {
    assert_case_matches_reference(&["app", "--flag"], &[("MODE", "test"), ("EMPTY", "")]);
}

#[test]
fn empty_args_and_environment_match_wasmtime_wasi() {
    assert_case_matches_reference(&[], &[]);
}
