use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance as MiniInstance, MemoryHandle, Value};
use wasm_wasi::{WasiPreview1, ERRNO_SUCCESS};
use wasmtime::{Engine, Linker, Memory, MemoryType, Module as ReferenceModule, Store};
use wasmtime_wasi::{
    p1::{self, WasiP1Ctx},
    p2::pipe::{MemoryInputPipe, MemoryOutputPipe},
    WasiCtxBuilder,
};

const SNAPSHOT_BYTES: usize = 192;
const READ_IOV_1: usize = 0;
const NREAD_1: usize = 8;
const READ_IOV_2: usize = 16;
const NREAD_2: usize = 24;
const READ_IOV_3: usize = 32;
const NREAD_3: usize = 40;
const WRITE_IOVS: usize = 48;
const NWRITTEN: usize = 64;
const READ_BUFFER_1: usize = 96;
const READ_BUFFER_2: usize = 112;
const READ_BUFFER_3: usize = 128;
const WRITE_PAYLOAD_1: usize = 144;
const WRITE_PAYLOAD_2: usize = 160;
const STDIN: &[u8] = b"abcdef";
const STDOUT: &[u8] = b"ping pong";

#[derive(Debug, Clone, PartialEq, Eq)]
struct DescriptorTrace {
    errnos: [i32; 4],
    memory: Vec<u8>,
    stdout: Vec<u8>,
}

fn module_bytes() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (import "wasi_snapshot_preview1" "fd_read"
                (func $fd_read (param i32 i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_write"
                (func $fd_write (param i32 i32 i32 i32) (result i32)))
            (import "env" "memory" (memory 1 1))
            (export "memory" (memory 0))

            (func (export "read_first") (result i32)
                i32.const 0
                i32.const 0
                i32.const 1
                i32.const 8
                call $fd_read)
            (func (export "read_second") (result i32)
                i32.const 0
                i32.const 16
                i32.const 1
                i32.const 24
                call $fd_read)
            (func (export "read_eof") (result i32)
                i32.const 0
                i32.const 32
                i32.const 1
                i32.const 40
                call $fd_read)
            (func (export "write_stdout") (result i32)
                i32.const 1
                i32.const 48
                i32.const 2
                i32.const 64
                call $fd_write))
        "#,
    )
    .expect("compile deterministic WASI descriptor-I/O module")
}

fn write_u32(memory: &mut [u8], address: usize, value: u32) {
    memory[address..address + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_iovec(memory: &mut [u8], address: usize, pointer: usize, length: usize) {
    write_u32(memory, address, pointer as u32);
    write_u32(memory, address + 4, length as u32);
}

fn seeded_memory() -> Vec<u8> {
    let mut memory = vec![0_u8; SNAPSHOT_BYTES];
    write_iovec(&mut memory, READ_IOV_1, READ_BUFFER_1, 4);
    write_iovec(&mut memory, READ_IOV_2, READ_BUFFER_2, 4);
    write_iovec(&mut memory, READ_IOV_3, READ_BUFFER_3, 4);
    write_iovec(&mut memory, WRITE_IOVS, WRITE_PAYLOAD_1, 4);
    write_iovec(&mut memory, WRITE_IOVS + 8, WRITE_PAYLOAD_2, 5);
    memory[WRITE_PAYLOAD_1..WRITE_PAYLOAD_1 + 4].copy_from_slice(b"ping");
    memory[WRITE_PAYLOAD_2..WRITE_PAYLOAD_2 + 5].copy_from_slice(b" pong");
    memory
}

fn mini_errno(instance: &mut MiniInstance, export: &str) -> i32 {
    match instance
        .invoke_export(export, &[])
        .unwrap_or_else(|error| panic!("mini WASI descriptor call {export:?} trapped: {error:?}"))
    {
        Some(Value::I32(errno)) => errno,
        other => panic!("mini WASI descriptor call {export:?} returned {other:?}"),
    }
}

fn run_mini(bytes: &[u8]) -> DescriptorTrace {
    let initial = seeded_memory();
    let memory = MemoryHandle::new(1, Some(1)).expect("create mini WASI descriptor memory");
    memory
        .write(0, &initial)
        .expect("seed mini WASI descriptor memory");
    let wasi = WasiPreview1::new().with_stdin(STDIN);
    let stdout = wasi.stdout();
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "memory", memory.clone())
        .expect("register mini WASI descriptor memory");
    wasi.register(&mut hosts)
        .expect("register mini WASI Preview1 descriptor surface");
    let module = parse_module(bytes).expect("descriptor-I/O module must parse in mini runtime");
    let mut instance = MiniInstance::with_hosts(module, hosts)
        .expect("descriptor-I/O module must instantiate in mini runtime");

    let errnos = [
        mini_errno(&mut instance, "read_first"),
        mini_errno(&mut instance, "read_second"),
        mini_errno(&mut instance, "read_eof"),
        mini_errno(&mut instance, "write_stdout"),
    ];
    DescriptorTrace {
        errnos,
        memory: memory
            .read(0, SNAPSHOT_BYTES)
            .expect("read mini descriptor-I/O memory snapshot"),
        stdout: stdout.snapshot(),
    }
}

fn reference_errno(
    instance: &wasmtime::Instance,
    store: &mut Store<WasiP1Ctx>,
    export: &str,
) -> i32 {
    instance
        .get_typed_func::<(), i32>(&mut *store, export)
        .unwrap_or_else(|error| panic!("resolve Wasmtime descriptor export {export:?}: {error}"))
        .call(&mut *store, ())
        .unwrap_or_else(|error| panic!("Wasmtime descriptor call {export:?} trapped: {error}"))
}

fn run_reference(engine: &Engine, bytes: &[u8]) -> DescriptorTrace {
    let module = ReferenceModule::new(engine, bytes)
        .expect("compile WASI descriptor-I/O module in Wasmtime");
    let stdin = MemoryInputPipe::new(STDIN.to_vec());
    let stdout = MemoryOutputPipe::new(1024);
    let mut builder = WasiCtxBuilder::new();
    builder.stdin(stdin).stdout(stdout.clone());
    let mut store = Store::new(engine, builder.build_p1());
    let memory = Memory::new(&mut store, MemoryType::new(1, Some(1)))
        .expect("create Wasmtime WASI descriptor memory");
    memory
        .write(&mut store, 0, &seeded_memory())
        .expect("seed Wasmtime WASI descriptor memory");
    let mut linker: Linker<WasiP1Ctx> = Linker::new(engine);
    p1::add_to_linker_sync(&mut linker, |context| context)
        .expect("register Wasmtime WASI Preview1 descriptor imports");
    linker
        .define(&store, "env", "memory", memory)
        .expect("register Wasmtime descriptor memory");
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiate descriptor-I/O module in Wasmtime");

    let errnos = [
        reference_errno(&instance, &mut store, "read_first"),
        reference_errno(&instance, &mut store, "read_second"),
        reference_errno(&instance, &mut store, "read_eof"),
        reference_errno(&instance, &mut store, "write_stdout"),
    ];
    let mut snapshot = vec![0_u8; SNAPSHOT_BYTES];
    memory
        .read(&store, 0, &mut snapshot)
        .expect("read Wasmtime descriptor-I/O memory snapshot");
    DescriptorTrace {
        errnos,
        memory: snapshot,
        stdout: stdout.contents().to_vec(),
    }
}

fn expected_trace() -> DescriptorTrace {
    let mut memory = seeded_memory();
    memory[READ_BUFFER_1..READ_BUFFER_1 + 4].copy_from_slice(b"abcd");
    memory[READ_BUFFER_2..READ_BUFFER_2 + 2].copy_from_slice(b"ef");
    write_u32(&mut memory, NREAD_1, 4);
    write_u32(&mut memory, NREAD_2, 2);
    write_u32(&mut memory, NREAD_3, 0);
    write_u32(&mut memory, NWRITTEN, STDOUT.len() as u32);
    DescriptorTrace {
        errnos: [ERRNO_SUCCESS; 4],
        memory,
        stdout: STDOUT.to_vec(),
    }
}

#[test]
fn deterministic_descriptor_io_trace_matches_wasmtime_wasi() {
    let bytes = module_bytes();
    let mini = run_mini(&bytes);
    let reference = run_reference(&Engine::default(), &bytes);
    let expected = expected_trace();

    assert_eq!(mini, expected, "mini WASI descriptor-I/O trace mismatch");
    assert_eq!(
        reference, expected,
        "Wasmtime WASI descriptor-I/O trace mismatch"
    );
    assert_eq!(mini, reference, "WASI descriptor-I/O differential mismatch");
}
