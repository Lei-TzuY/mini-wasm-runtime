use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"(module
  (memory 1)
  (func (export "abs") (result i32) i32.const 0 v128.const f32x4 -0.0 -1.5 9 16 f32x4.abs v128.store i32.const 0 i32.load offset=4)
  (func (export "neg") (result i32) i32.const 0 v128.const f32x4 0.0 1.5 -9 -16 f32x4.neg v128.store i32.const 0 i32.load)
  (func (export "sqrt") (result i32) i32.const 0 v128.const f32x4 1 4 9 16 f32x4.sqrt v128.store i32.const 0 i32.load offset=8)
  (func (export "ceil") (result i32) i32.const 0 v128.const f32x4 -1.5 -0.0 1.25 2 f32x4.ceil v128.store i32.const 0 i32.load)
  (func (export "floor") (result i32) i32.const 0 v128.const f32x4 -1.5 -0.0 1.25 2 f32x4.floor v128.store i32.const 0 i32.load)
  (func (export "trunc") (result i32) i32.const 0 v128.const f32x4 -1.5 -0.0 1.75 2 f32x4.trunc v128.store i32.const 0 i32.load)
  (func (export "nearest") (result i32) i32.const 0 v128.const f32x4 -2.5 1.5 2.5 -0.5 f32x4.nearest v128.store i32.const 0 i32.load))"#;
const EXPORTS: [&str; 7] = ["abs", "neg", "sqrt", "ceil", "floor", "trunc", "nearest"];
fn mini_trace(bytes: &[u8]) -> Vec<i32> {
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
                [Value::I32(value)] => *value,
                other => panic!("unexpected mini result for {export}: {other:?}"),
            }
        })
        .collect()
}
fn reference_trace(bytes: &[u8]) -> Vec<i32> {
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
                .get_typed_func::<(), i32>(&mut store, export)
                .expect("signature")
                .call(&mut store, ())
                .expect("reference execution")
        })
        .collect()
}
#[test]
fn f32x4_unary_matches_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("fixture parse");
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, reference);
    assert_eq!(mini[0] as u32, 1.5f32.to_bits());
    assert_eq!(mini[1] as u32, (-0.0f32).to_bits());
    assert_eq!(mini[2] as u32, 3.0f32.to_bits());
    assert_eq!(mini[3] as u32, (-1.0f32).to_bits());
    assert_eq!(mini[4] as u32, (-2.0f32).to_bits());
    assert_eq!(mini[5] as u32, (-1.0f32).to_bits());
    assert_eq!(mini[6] as u32, (-2.0f32).to_bits());
}
