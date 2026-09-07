use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (memory i64 1 2)
  (func (export "size") (result i64)
    memory.size)
  (func (export "grow") (result i64)
    i64.const 1
    memory.grow)
  (func (export "store_load") (result i32)
    i64.const 65540
    i32.const 305419896
    i32.store
    i64.const 65540
    i32.load)
  (func (export "large_offset") (result i32)
    i64.const 0
    i32.load offset=4294967296))
"#;

fn mini_trace(bytes: &[u8]) -> Vec<i64> {
    let module = parse_module(bytes).expect("mini runtime must parse memory64 fixture");
    let mut instance =
        MiniInstance::new(module).expect("mini runtime must instantiate memory64 fixture");

    let mut trace = Vec::new();
    for export in ["size", "grow", "size"] {
        let values = instance
            .invoke_export_values(export, &[])
            .expect("mini runtime memory64 execution must succeed");
        match values.as_slice() {
            [Value::I64(value)] => trace.push(*value),
            other => panic!("unexpected mini memory64 result for {export}: {other:?}"),
        }
    }
    let values = instance
        .invoke_export_values("store_load", &[])
        .expect("mini runtime i64-addressed memory access must succeed");
    match values.as_slice() {
        [Value::I32(value)] => trace.push(i64::from(*value)),
        other => panic!("unexpected mini memory64 load result: {other:?}"),
    }
    trace
}

fn mini_large_offset_traps(bytes: &[u8]) -> bool {
    let module = parse_module(bytes).expect("mini runtime must parse large memory64 memarg");
    let mut instance =
        MiniInstance::new(module).expect("mini runtime must validate large memory64 memarg");
    instance.invoke_export_values("large_offset", &[]).is_err()
}

fn reference_engine() -> Engine {
    let mut config = Config::new();
    config.wasm_memory64(true);
    Engine::new(&config).expect("memory64-enabled Wasmtime engine must initialize")
}

fn reference_trace(bytes: &[u8]) -> Vec<i64> {
    let engine = reference_engine();
    let module =
        ReferenceModule::new(&engine, bytes).expect("Wasmtime must compile memory64 fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate memory64 fixture");

    let size = instance
        .get_typed_func::<(), i64>(&mut store, "size")
        .expect("size export must be [] -> [i64]");
    let grow = instance
        .get_typed_func::<(), i64>(&mut store, "grow")
        .expect("grow export must be [] -> [i64]");
    let store_load = instance
        .get_typed_func::<(), i32>(&mut store, "store_load")
        .expect("store_load export must be [] -> [i32]");

    vec![
        size.call(&mut store, ())
            .expect("initial memory.size must succeed"),
        grow.call(&mut store, ()).expect("memory.grow must succeed"),
        size.call(&mut store, ())
            .expect("grown memory.size must succeed"),
        i64::from(
            store_load
                .call(&mut store, ())
                .expect("i64-addressed store/load must succeed"),
        ),
    ]
}

fn reference_large_offset_traps(bytes: &[u8]) -> bool {
    let engine = reference_engine();
    let module =
        ReferenceModule::new(&engine, bytes).expect("Wasmtime must compile large memory64 memarg");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate large memory64 memarg fixture");
    let large_offset = instance
        .get_typed_func::<(), i32>(&mut store, "large_offset")
        .expect("large_offset export must be [] -> [i32]");
    large_offset.call(&mut store, ()).is_err()
}

#[test]
fn memory64_address_width_matches_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("memory64 WAT fixture must parse");
    let expected = vec![1, 1, 2, 305_419_896];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);

    assert_eq!(mini, expected, "mini memory64 trace drifted");
    assert_eq!(reference, expected, "Wasmtime memory64 trace drifted");
    assert_eq!(mini, reference, "memory64 traces diverged");
    assert!(
        mini_large_offset_traps(&bytes),
        "mini must validate u64 static offset and trap only at runtime bounds"
    );
    assert!(
        reference_large_offset_traps(&bytes),
        "Wasmtime must likewise accept the u64 static offset and trap on access"
    );
}
