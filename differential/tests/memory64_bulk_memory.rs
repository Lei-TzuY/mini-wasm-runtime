use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (memory i64 1 1)
  (func (export "fill") (result i32)
    i64.const 3
    i32.const 90
    i64.const 4
    memory.fill
    i64.const 6
    i32.load8_u)
  (func (export "copy") (result i32)
    i64.const 9
    i64.const 3
    i64.const 4
    memory.copy
    i64.const 12
    i32.load8_u)
  (func (export "large_fill")
    i64.const 4294967296
    i32.const 17
    i64.const 1
    memory.fill))
"#;

fn mini_trace(bytes: &[u8]) -> (Vec<i32>, bool) {
    let module = parse_module(bytes).expect("mini runtime must parse memory64 bulk fixture");
    let mut instance =
        MiniInstance::new(module).expect("mini runtime must instantiate memory64 bulk fixture");

    let mut trace = Vec::new();
    for export in ["fill", "copy"] {
        let values = instance
            .invoke_export_values(export, &[])
            .expect("mini runtime memory64 bulk execution must succeed");
        match values.as_slice() {
            [Value::I32(value)] => trace.push(*value),
            other => panic!("unexpected mini memory64 bulk result for {export}: {other:?}"),
        }
    }
    let large_fill_traps = instance.invoke_export_values("large_fill", &[]).is_err();
    (trace, large_fill_traps)
}

fn reference_engine() -> Engine {
    let mut config = Config::new();
    config.wasm_memory64(true);
    Engine::new(&config).expect("memory64-enabled Wasmtime engine must initialize")
}

fn reference_trace(bytes: &[u8]) -> (Vec<i32>, bool) {
    let engine = reference_engine();
    let module =
        ReferenceModule::new(&engine, bytes).expect("Wasmtime must compile memory64 bulk fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate memory64 bulk fixture");

    let fill = instance
        .get_typed_func::<(), i32>(&mut store, "fill")
        .expect("fill export must be [] -> [i32]");
    let copy = instance
        .get_typed_func::<(), i32>(&mut store, "copy")
        .expect("copy export must be [] -> [i32]");
    let large_fill = instance
        .get_typed_func::<(), ()>(&mut store, "large_fill")
        .expect("large_fill export must be [] -> []");

    let trace = vec![
        fill.call(&mut store, ())
            .expect("memory64 memory.fill must succeed"),
        copy.call(&mut store, ())
            .expect("memory64 memory.copy must succeed"),
    ];
    let large_fill_traps = large_fill.call(&mut store, ()).is_err();
    (trace, large_fill_traps)
}

#[test]
fn memory64_bulk_memory_matches_wasmtime_reference_without_large_allocation() {
    let bytes = wat::parse_str(FIXTURE).expect("memory64 bulk WAT fixture must parse");
    let expected = vec![90, 90];
    let (mini, mini_large_fill_traps) = mini_trace(&bytes);
    let (reference, reference_large_fill_traps) = reference_trace(&bytes);

    assert_eq!(mini, expected, "mini memory64 bulk trace drifted");
    assert_eq!(reference, expected, "Wasmtime memory64 bulk trace drifted");
    assert_eq!(mini, reference, "memory64 bulk traces diverged");
    assert!(
        mini_large_fill_traps,
        "mini must preserve a 2^32 bulk destination through runtime bounds preflight"
    );
    assert!(
        reference_large_fill_traps,
        "Wasmtime must likewise trap the 2^32 bulk destination against one bounded page"
    );
}
