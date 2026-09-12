use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "a") (result i32)
    v128.const f32x4 2 -3 0.5 4
    v128.const f32x4 3 2 8 -0.5
    v128.const f32x4 1 1 -1 5
    f32x4.relaxed_madd
    i32x4.extract_lane 0)
  (func (export "b") (result i32)
    v128.const f32x4 2 -3 0.5 4
    v128.const f32x4 3 2 8 -0.5
    v128.const f32x4 1 1 -1 5
    f32x4.relaxed_madd
    i32x4.extract_lane 3))
"#;
const EXPORTS: [&str; 2] = ["a", "b"];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini runtime must parse relaxed madd fixture");
    let mut instance = MiniInstance::new(module).expect("mini runtime must instantiate relaxed madd fixture");
    EXPORTS
        .into_iter()
        .map(|export| match instance
            .invoke_export_values(export, &[])
            .expect("mini relaxed madd execution must succeed")
            .as_slice()
        {
            [Value::I32(value)] => *value,
            other => panic!("unexpected mini result for {export}: {other:?}"),
        })
        .collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i32> {
    let mut config = Config::new();
    config.wasm_simd(true);
    config.wasm_relaxed_simd(true);
    let engine = Engine::new(&config).expect("relaxed-SIMD Wasmtime engine must initialize");
    let module = ReferenceModule::new(&engine, bytes).expect("Wasmtime must compile relaxed madd fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate relaxed madd fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i32>(&mut store, export)
                .expect("relaxed madd export must be [] -> [i32]")
                .call(&mut store, ())
                .expect("Wasmtime relaxed madd execution must succeed")
        })
        .collect()
}

#[test]
fn relaxed_madd_matches_wasmtime_on_exact_finite_lanes() {
    let bytes = wat::parse_str(FIXTURE).expect("relaxed madd WAT fixture must parse");
    let expected = vec![7.0f32.to_bits() as i32, 3.0f32.to_bits() as i32];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, expected);
    assert_eq!(reference, expected);
    assert_eq!(mini, reference);
}
