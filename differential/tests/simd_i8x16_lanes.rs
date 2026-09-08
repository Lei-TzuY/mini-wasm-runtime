use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "signed") (result i32)
    i32.const -128
    i8x16.splat
    i8x16.extract_lane_s 7)
  (func (export "unsigned") (result i32)
    i32.const -128
    i8x16.splat
    i8x16.extract_lane_u 12)
  (func (export "replace") (result i32)
    i32.const 7
    i8x16.splat
    i32.const 511
    i8x16.replace_lane 15
    i8x16.extract_lane_u 15)
  (func (export "preserve") (result i32)
    i32.const 7
    i8x16.splat
    i32.const -1
    i8x16.replace_lane 15
    i8x16.extract_lane_u 0))
"#;

const EXPORTS: [&str; 4] = ["signed", "unsigned", "replace", "preserve"];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini runtime must parse i8x16 lane fixture");
    let mut instance =
        MiniInstance::new(module).expect("mini runtime must instantiate i8x16 lane fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            let values = instance
                .invoke_export_values(export, &[])
                .expect("mini i8x16 lane execution must succeed");
            match values.as_slice() {
                [Value::I32(value)] => *value,
                other => panic!("unexpected mini i8x16 result for {export}: {other:?}"),
            }
        })
        .collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i32> {
    let mut config = Config::new();
    config.wasm_simd(true);
    let engine = Engine::new(&config).expect("SIMD-enabled Wasmtime engine must initialize");
    let module =
        ReferenceModule::new(&engine, bytes).expect("Wasmtime must compile i8x16 lane fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate i8x16 lane fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i32>(&mut store, export)
                .expect("i8x16 lane export must be [] -> [i32]")
                .call(&mut store, ())
                .expect("Wasmtime i8x16 lane execution must succeed")
        })
        .collect()
}

#[test]
fn i8x16_lane_primitives_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("i8x16 lane WAT fixture must parse");
    let expected = vec![-128, 128, 255, 7];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, expected, "mini i8x16 lane trace drifted");
    assert_eq!(reference, expected, "Wasmtime i8x16 lane trace drifted");
    assert_eq!(mini, reference, "i8x16 lane traces diverged");
}
