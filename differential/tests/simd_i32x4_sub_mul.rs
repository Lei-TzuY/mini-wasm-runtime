use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "sub_wrap") (result i32)
    v128.const i32x4 -2147483648 7 -9 12
    v128.const i32x4 1 10 -4 2
    i32x4.sub
    i32x4.extract_lane 0)
  (func (export "mul_wrap") (result i32)
    v128.const i32x4 1073741824 -3 7 11
    v128.const i32x4 4 9 -5 13
    i32x4.mul
    i32x4.extract_lane 0))
"#;

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini runtime must parse SIMD fixture");
    let mut instance =
        MiniInstance::new(module).expect("mini runtime must instantiate SIMD fixture");
    ["sub_wrap", "mul_wrap"]
        .into_iter()
        .map(|export| {
            match instance
                .invoke_export_values(export, &[])
                .expect("mini SIMD execution must succeed")
                .as_slice()
            {
                [Value::I32(value)] => *value,
                other => panic!("unexpected mini SIMD result for {export}: {other:?}"),
            }
        })
        .collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i32> {
    let mut config = Config::new();
    config.wasm_simd(true);
    let engine = Engine::new(&config).expect("SIMD-enabled Wasmtime engine must initialize");
    let module = ReferenceModule::new(&engine, bytes).expect("Wasmtime must compile SIMD fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate SIMD fixture");
    ["sub_wrap", "mul_wrap"]
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
fn i32x4_sub_mul_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("SIMD WAT fixture must parse");
    let expected = vec![i32::MAX, 0];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, expected, "mini SIMD trace drifted");
    assert_eq!(reference, expected, "Wasmtime SIMD trace drifted");
    assert_eq!(mini, reference, "SIMD traces diverged");
}
