use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "a") (result i32)
    v128.const f64x2 1.75 -12345.75
    i32x4.relaxed_trunc_f64x2_s_zero
    i32x4.extract_lane 0)
  (func (export "b") (result i32)
    v128.const f64x2 1.75 -12345.75
    i32x4.relaxed_trunc_f64x2_s_zero
    i32x4.extract_lane 1)
  (func (export "z2") (result i32)
    v128.const f64x2 1.75 -12345.75
    i32x4.relaxed_trunc_f64x2_s_zero
    i32x4.extract_lane 2)
  (func (export "z3") (result i32)
    v128.const f64x2 1.75 -12345.75
    i32x4.relaxed_trunc_f64x2_s_zero
    i32x4.extract_lane 3))
"#;
const EXPORTS: [&str; 4] = ["a", "b", "z2", "z3"];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini parses fixture");
    let mut instance = MiniInstance::new(module).expect("mini instantiates fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            match instance
                .invoke_export_values(export, &[])
                .expect("mini executes")
                .as_slice()
            {
                [Value::I32(value)] => *value,
                other => panic!("unexpected mini result for {export}: {other:?}"),
            }
        })
        .collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i32> {
    let mut config = Config::new();
    config.wasm_simd(true);
    config.wasm_relaxed_simd(true);
    let engine = Engine::new(&config).expect("engine initializes");
    let module = ReferenceModule::new(&engine, bytes).expect("Wasmtime compiles fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[]).expect("Wasmtime instantiates");
    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i32>(&mut store, export)
                .expect("typed export")
                .call(&mut store, ())
                .expect("Wasmtime executes")
        })
        .collect()
}

#[test]
fn matches_wasmtime_on_deterministic_lanes() {
    let bytes = wat::parse_str(FIXTURE).expect("WAT parses");
    let expected = vec![1, -12345, 0, 0];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, expected);
    assert_eq!(reference, expected);
    assert_eq!(mini, reference);
}
