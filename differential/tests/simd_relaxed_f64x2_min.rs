use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (memory 1)
  (func (export "ordered") (result i64)
    i32.const 0
    v128.const f64x2 3 -2
    v128.const f64x2 4 -5
    f64x2.relaxed_min
    v128.store
    i32.const 0
    i64.load))
"#;

#[test]
fn relaxed_f64x2_min_matches_wasmtime_for_ordered_lanes() {
    let bytes = wat::parse_str(FIXTURE).expect("relaxed min WAT parses");
    let parsed = parse_module(&bytes).expect("mini parses relaxed min fixture");
    let mut mini = MiniInstance::new(parsed).expect("mini instantiates relaxed min fixture");
    let mini_value = match mini
        .invoke_export_values("ordered", &[])
        .unwrap()
        .as_slice()
    {
        [Value::I64(v)] => *v,
        other => panic!("unexpected mini result: {other:?}"),
    };

    let mut config = Config::new();
    config.wasm_simd(true);
    config.wasm_relaxed_simd(true);
    let engine = Engine::new(&config).expect("Wasmtime engine initializes");
    let module = ReferenceModule::new(&engine, &bytes).expect("Wasmtime compiles relaxed min");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[]).expect("Wasmtime instantiates");
    let reference = instance
        .get_typed_func::<(), i64>(&mut store, "ordered")
        .unwrap()
        .call(&mut store, ())
        .unwrap();

    assert_eq!(mini_value, 3.0f64.to_bits() as i64);
    assert_eq!(mini_value, reference);
}
