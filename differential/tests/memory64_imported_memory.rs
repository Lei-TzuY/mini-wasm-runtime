use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance as MiniInstance, MemoryHandle, Value};
use wasmtime::{
    Config, Engine, Extern, Instance as ReferenceInstance, Memory as ReferenceMemory,
    MemoryType as ReferenceMemoryType, Module as ReferenceModule, Store,
};

const FIXTURE: &str = r#"
(module
  (import "env" "mem" (memory i64 1 2))
  (func (export "load") (param i64) (result i32)
    local.get 0
    i32.load)
  (func (export "store") (param i64 i32)
    local.get 0
    local.get 1
    i32.store)
  (func (export "fill") (param i64 i32 i64)
    local.get 0
    local.get 1
    local.get 2
    memory.fill)
  (func (export "size") (result i64)
    memory.size)
  (func (export "grow") (param i64) (result i64)
    local.get 0
    memory.grow))
"#;

#[derive(Debug, PartialEq, Eq)]
struct Trace {
    initial_load: i32,
    size_before: i64,
    grow_result: i64,
    size_after: i64,
    bytes_after: Vec<u8>,
    high_address_trapped: bool,
}

fn mini_trace(bytes: &[u8]) -> Trace {
    let module = parse_module(bytes).expect("mini runtime must parse imported memory64 fixture");
    let memory = MemoryHandle::new64(1, Some(2)).expect("create bounded memory64 handle");
    memory
        .write(8, &123i32.to_le_bytes())
        .expect("initialize shared mini memory");
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "mem", memory.clone())
        .expect("register memory64 import");
    let mut instance = MiniInstance::with_hosts(module, hosts)
        .expect("mini runtime must bind imported memory64 handle");

    let initial_load = match instance
        .invoke_export_values("load", &[Value::I64(8)])
        .expect("mini i64-addressed load must succeed")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected mini load result: {other:?}"),
    };
    instance
        .invoke_export_values("store", &[Value::I64(12), Value::I32(77)])
        .expect("mini i64-addressed store must succeed");
    instance
        .invoke_export_values("fill", &[Value::I64(16), Value::I32(0xab), Value::I64(3)])
        .expect("mini memory64 fill must succeed");

    let size_before = match instance
        .invoke_export_values("size", &[])
        .expect("mini memory.size must succeed")
        .as_slice()
    {
        [Value::I64(value)] => *value,
        other => panic!("unexpected mini size result: {other:?}"),
    };
    let grow_result = match instance
        .invoke_export_values("grow", &[Value::I64(1)])
        .expect("mini memory.grow must succeed")
        .as_slice()
    {
        [Value::I64(value)] => *value,
        other => panic!("unexpected mini grow result: {other:?}"),
    };
    let size_after = match instance
        .invoke_export_values("size", &[])
        .expect("mini grown memory.size must succeed")
        .as_slice()
    {
        [Value::I64(value)] => *value,
        other => panic!("unexpected mini grown size result: {other:?}"),
    };
    let bytes_after = memory.read(8, 11).expect("read shared mini memory");
    let high_address_trapped = instance
        .invoke_export_values("load", &[Value::I64(1_i64 << 32)])
        .is_err();

    Trace {
        initial_load,
        size_before,
        grow_result,
        size_after,
        bytes_after,
        high_address_trapped,
    }
}

fn reference_engine() -> Engine {
    let mut config = Config::new();
    config.wasm_memory64(true);
    Engine::new(&config).expect("memory64-enabled Wasmtime engine must initialize")
}

fn reference_trace(bytes: &[u8]) -> Trace {
    let engine = reference_engine();
    let module = ReferenceModule::new(&engine, bytes)
        .expect("Wasmtime must compile imported memory64 fixture");
    let mut store = Store::new(&engine, ());
    let memory = ReferenceMemory::new(&mut store, ReferenceMemoryType::new64(1, Some(2)))
        .expect("create bounded Wasmtime memory64 import");
    memory
        .write(&mut store, 8, &123i32.to_le_bytes())
        .expect("initialize Wasmtime memory64 import");
    let instance = ReferenceInstance::new(&mut store, &module, &[Extern::Memory(memory)])
        .expect("Wasmtime must bind imported memory64");

    let load = instance
        .get_typed_func::<i64, i32>(&mut store, "load")
        .expect("load export must be [i64] -> [i32]");
    let store_value = instance
        .get_typed_func::<(i64, i32), ()>(&mut store, "store")
        .expect("store export must be [i64, i32] -> []");
    let fill = instance
        .get_typed_func::<(i64, i32, i64), ()>(&mut store, "fill")
        .expect("fill export must be [i64, i32, i64] -> []");
    let size = instance
        .get_typed_func::<(), i64>(&mut store, "size")
        .expect("size export must be [] -> [i64]");
    let grow = instance
        .get_typed_func::<i64, i64>(&mut store, "grow")
        .expect("grow export must be [i64] -> [i64]");

    let initial_load = load
        .call(&mut store, 8)
        .expect("reference load must succeed");
    store_value
        .call(&mut store, (12, 77))
        .expect("reference store must succeed");
    fill.call(&mut store, (16, 0xab, 3))
        .expect("reference fill must succeed");
    let size_before = size
        .call(&mut store, ())
        .expect("reference size must succeed");
    let grow_result = grow
        .call(&mut store, 1)
        .expect("reference grow must succeed");
    let size_after = size
        .call(&mut store, ())
        .expect("reference grown size must succeed");
    let mut bytes_after = vec![0; 11];
    memory
        .read(&store, 8, &mut bytes_after)
        .expect("read shared Wasmtime memory64 import");
    let high_address_trapped = load.call(&mut store, 1_i64 << 32).is_err();

    Trace {
        initial_load,
        size_before,
        grow_result,
        size_after,
        bytes_after,
        high_address_trapped,
    }
}

#[test]
fn imported_memory64_shared_backing_matches_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("imported memory64 WAT fixture must parse");
    let expected = Trace {
        initial_load: 123,
        size_before: 1,
        grow_result: 1,
        size_after: 2,
        bytes_after: vec![123, 0, 0, 0, 77, 0, 0, 0, 0xab, 0xab, 0xab],
        high_address_trapped: true,
    };
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);

    assert_eq!(mini, expected, "mini imported-memory64 trace drifted");
    assert_eq!(
        reference, expected,
        "Wasmtime imported-memory64 trace drifted"
    );
    assert_eq!(mini, reference, "imported-memory64 traces diverged");
}
