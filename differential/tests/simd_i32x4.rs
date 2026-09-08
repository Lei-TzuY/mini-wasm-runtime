use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "lane") (result i32)
    (block (result i32)
      v128.const i32x4 1 2 3 4
      i32.const 10
      i32x4.splat
      i32x4.add
      i32x4.extract_lane 2))
  (func (export "wrap") (result i32)
    v128.const i32x4 2147483647 0 0 0
    i32.const 1
    i32x4.splat
    i32x4.add
    i32x4.extract_lane 0))
"#;

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini runtime must parse SIMD fixture");
    let mut instance =
        MiniInstance::new(module).expect("mini runtime must instantiate SIMD fixture");

    ["lane", "wrap"]
        .into_iter()
        .map(|export| {
            let values = instance
                .invoke_export_values(export, &[])
                .expect("mini SIMD execution must succeed");
            match values.as_slice() {
                [Value::I32(value)] => *value,
                other => panic!("unexpected mini SIMD result for {export}: {other:?}"),
            }
        })
        .collect()
}

fn reference_engine() -> Engine {
    let mut config = Config::new();
    config.wasm_simd(true);
    Engine::new(&config).expect("SIMD-enabled Wasmtime engine must initialize")
}

fn reference_trace(bytes: &[u8]) -> Vec<i32> {
    let engine = reference_engine();
    let module = ReferenceModule::new(&engine, bytes).expect("Wasmtime must compile SIMD fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate SIMD fixture");

    ["lane", "wrap"]
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i32>(&mut store, export)
                .expect("SIMD export must be [] -> [i32]")
                .call(&mut store, ())
                .expect("Wasmtime SIMD execution must succeed")
        })
        .collect()
}

#[test]
fn initial_i32x4_semantics_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("SIMD WAT fixture must parse");
    let expected = vec![13, i32::MIN];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);

    assert_eq!(mini, expected, "mini SIMD trace drifted");
    assert_eq!(reference, expected, "Wasmtime SIMD trace drifted");
    assert_eq!(mini, reference, "SIMD traces diverged");
}
