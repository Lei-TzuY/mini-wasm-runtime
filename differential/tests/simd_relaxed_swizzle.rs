use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "in_range") (result i32)
    v128.const i8x16 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25
    v128.const i8x16 15 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14
    i8x16.relaxed_swizzle
    i8x16.extract_lane_u 0)
  (func (export "high_oob") (result i32)
    v128.const i8x16 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25
    v128.const i8x16 -128 -1 0 1 2 3 4 5 6 7 8 9 10 11 12 13
    i8x16.relaxed_swizzle
    i8x16.extract_lane_u 0))
"#;

const EXPORTS: [&str; 2] = ["in_range", "high_oob"];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini runtime must parse relaxed swizzle fixture");
    let mut instance =
        MiniInstance::new(module).expect("mini runtime must instantiate relaxed swizzle fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            match instance
                .invoke_export_values(export, &[])
                .expect("mini relaxed swizzle execution must succeed")
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
    let engine = Engine::new(&config).expect("relaxed-SIMD Wasmtime engine must initialize");
    let module = ReferenceModule::new(&engine, bytes)
        .expect("Wasmtime must compile relaxed swizzle fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate relaxed swizzle fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i32>(&mut store, export)
                .expect("relaxed swizzle export must be [] -> [i32]")
                .call(&mut store, ())
                .expect("Wasmtime relaxed swizzle execution must succeed")
        })
        .collect()
}

#[test]
fn relaxed_swizzle_matches_wasmtime_on_deterministic_lanes() {
    let bytes = wat::parse_str(FIXTURE).expect("relaxed swizzle WAT fixture must parse");
    let expected = vec![25, 0];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(
        mini, expected,
        "mini relaxed swizzle deterministic lanes drifted"
    );
    assert_eq!(
        reference, expected,
        "Wasmtime relaxed swizzle deterministic lanes drifted"
    );
    assert_eq!(
        mini, reference,
        "relaxed swizzle deterministic traces diverged"
    );
}
