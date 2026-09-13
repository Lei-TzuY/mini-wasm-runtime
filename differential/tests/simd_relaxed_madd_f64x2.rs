use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (memory 1)
  (func (export "a") (result i64)
    i32.const 0
    v128.const f64x2 2 -3
    v128.const f64x2 3 2
    v128.const f64x2 1 8
    f64x2.relaxed_madd
    v128.store
    i32.const 0
    i64.load)
  (func (export "b") (result i64)
    i32.const 0
    v128.const f64x2 2 -3
    v128.const f64x2 3 2
    v128.const f64x2 1 8
    f64x2.relaxed_madd
    v128.store
    i32.const 8
    i64.load))
"#;
const EXPORTS: [&str; 2] = ["a", "b"];

fn mini_trace(bytes: &[u8]) -> Vec<i64> {
    let module = parse_module(bytes).expect("mini runtime must parse relaxed f64x2 madd fixture");
    let mut instance = MiniInstance::new(module).expect("mini runtime must instantiate relaxed f64x2 madd fixture");
    EXPORTS.into_iter().map(|export| match instance.invoke_export_values(export, &[]).expect("mini relaxed f64x2 madd execution must succeed").as_slice() {
        [Value::I64(value)] => *value,
        other => panic!("unexpected mini result for {export}: {other:?}"),
    }).collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i64> {
    let mut config = Config::new();
    config.wasm_simd(true);
    config.wasm_relaxed_simd(true);
    let engine = Engine::new(&config).expect("relaxed-SIMD Wasmtime engine must initialize");
    let module = ReferenceModule::new(&engine, bytes).expect("Wasmtime must compile relaxed f64x2 madd fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[]).expect("Wasmtime must instantiate relaxed f64x2 madd fixture");
    EXPORTS.into_iter().map(|export| instance.get_typed_func::<(), i64>(&mut store, export).expect("relaxed f64x2 madd export must be [] -> [i64]").call(&mut store, ()).expect("Wasmtime relaxed f64x2 madd execution must succeed")).collect()
}

#[test]
fn relaxed_f64x2_madd_matches_wasmtime_on_exact_finite_lanes() {
    let bytes = wat::parse_str(FIXTURE).expect("relaxed f64x2 madd WAT fixture must parse");
    let expected = vec![7.0f64.to_bits() as i64, 2.0f64.to_bits() as i64];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, expected);
    assert_eq!(reference, expected);
    assert_eq!(mini, reference);
}
