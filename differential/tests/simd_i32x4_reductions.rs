use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "all_nonzero") (result i32)
    v128.const i32x4 1 -2 3 -2147483648
    i32x4.all_true)
  (func (export "has_zero") (result i32)
    v128.const i32x4 1 0 -3 4
    i32x4.all_true)
  (func (export "mask") (result i32)
    v128.const i32x4 -1 0 -2147483648 2147483647
    i32x4.bitmask)
)
"#;

const EXPORTS: [&str; 3] = ["all_nonzero", "has_zero", "mask"];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini runtime must parse SIMD reduction fixture");
    let mut instance =
        MiniInstance::new(module).expect("mini runtime must instantiate SIMD reduction fixture");

    EXPORTS
        .into_iter()
        .map(|export| {
            let values = instance
                .invoke_export_values(export, &[])
                .expect("mini SIMD reduction execution must succeed");
            match values.as_slice() {
                [Value::I32(value)] => *value,
                other => panic!("unexpected mini SIMD reduction result for {export}: {other:?}"),
            }
        })
        .collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i32> {
    let mut config = Config::new();
    config.wasm_simd(true);
    let engine = Engine::new(&config).expect("SIMD-enabled Wasmtime engine must initialize");
    let module =
        ReferenceModule::new(&engine, bytes).expect("Wasmtime must compile SIMD reduction fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate SIMD reduction fixture");

    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i32>(&mut store, export)
                .expect("SIMD reduction export must be [] -> [i32]")
                .call(&mut store, ())
                .expect("Wasmtime SIMD reduction execution must succeed")
        })
        .collect()
}

#[test]
fn i32x4_reductions_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("SIMD reduction WAT fixture must parse");
    let expected = vec![1, 0, 0b0101];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);

    assert_eq!(mini, expected, "mini SIMD reduction trace drifted");
    assert_eq!(reference, expected, "Wasmtime SIMD reduction trace drifted");
    assert_eq!(mini, reference, "SIMD reduction traces diverged");
}
