use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"(module
  (memory 1)
  (func (export "add") (result i64) i32.const 0 v128.const f64x2 1.5 -8 v128.const f64x2 2.5 2 f64x2.add v128.store i32.const 0 i64.load)
  (func (export "sub") (result i64) i32.const 0 v128.const f64x2 1.5 -8 v128.const f64x2 2.5 2 f64x2.sub v128.store i32.const 0 i64.load offset=8)
  (func (export "mul") (result i64) i32.const 0 v128.const f64x2 6 -9 v128.const f64x2 -0.5 3 f64x2.mul v128.store i32.const 0 i64.load)
  (func (export "div") (result i64) i32.const 0 v128.const f64x2 6 -9 v128.const f64x2 -0.5 3 f64x2.div v128.store i32.const 0 i64.load offset=8))"#;
const EXPORTS: [&str; 4] = ["add", "sub", "mul", "div"];

fn mini_trace(bytes: &[u8]) -> Vec<i64> {
    let module = parse_module(bytes).expect("mini parse");
    let mut instance = MiniInstance::new(module).expect("mini instantiate");
    EXPORTS
        .into_iter()
        .map(|export| {
            match instance
                .invoke_export_values(export, &[])
                .expect("mini execution")
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
    let engine = Engine::new(&config).expect("SIMD engine");
    let module = ReferenceModule::new(&engine, bytes).expect("Wasmtime compile");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[]).expect("instantiate");
    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i64>(&mut store, export)
                .expect("signature")
                .call(&mut store, ())
                .expect("reference execution")
        })
        .collect()
}

#[test]
fn f64x2_binary_arithmetic_matches_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("fixture parse");
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, reference);
    assert_eq!(mini[0] as u64, 4.0f64.to_bits());
    assert_eq!(mini[1] as u64, (-10.0f64).to_bits());
    assert_eq!(mini[2] as u64, (-3.0f64).to_bits());
    assert_eq!(mini[3] as u64, (-3.0f64).to_bits());
}
