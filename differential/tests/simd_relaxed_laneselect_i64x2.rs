use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (memory 1)
  (func (export "all_a") (result i64)
    i32.const 0
    v128.const i64x2 -6148914691236517206 -6148914691236517206
    v128.const i64x2 6148914691236517205 6148914691236517205
    v128.const i64x2 -1 -1
    i64x2.relaxed_laneselect
    v128.store
    i32.const 0
    i64.load)
  (func (export "all_b") (result i64)
    i32.const 0
    v128.const i64x2 -6148914691236517206 -6148914691236517206
    v128.const i64x2 6148914691236517205 6148914691236517205
    v128.const i64x2 0 0
    i64x2.relaxed_laneselect
    v128.store
    i32.const 0
    i64.load))
"#;
const EXPORTS: [&str; 2] = ["all_a", "all_b"];

fn mini_trace(bytes: &[u8]) -> Vec<i64> {
    let module = parse_module(bytes).expect("mini runtime parses relaxed lane-select fixture");
    let mut instance =
        MiniInstance::new(module).expect("mini runtime instantiates relaxed lane-select fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            match instance
                .invoke_export_values(export, &[])
                .expect("mini lane-select executes")
                .as_slice()
            {
                [Value::I64(value)] => *value,
                other => panic!("unexpected mini result for {export}: {other:?}"),
            }
        })
        .collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i64> {
    let mut config = Config::new();
    config.wasm_simd(true);
    config.wasm_relaxed_simd(true);
    let engine = Engine::new(&config).expect("Wasmtime engine initializes");
    let module = ReferenceModule::new(&engine, bytes)
        .expect("Wasmtime compiles relaxed lane-select fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime instantiates relaxed lane-select fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i64>(&mut store, export)
                .expect("lane-select export is [] -> [i64]")
                .call(&mut store, ())
                .expect("Wasmtime lane-select executes")
        })
        .collect()
}

#[test]
fn relaxed_i64x2_laneselect_matches_wasmtime_for_deterministic_masks() {
    let bytes = wat::parse_str(FIXTURE).expect("relaxed lane-select WAT parses");
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, vec![-6148914691236517206, 6148914691236517205]);
    assert_eq!(reference, vec![-6148914691236517206, 6148914691236517205]);
    assert_eq!(mini, reference);
}
