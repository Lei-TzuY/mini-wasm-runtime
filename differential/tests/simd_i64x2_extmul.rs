use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"(module
  (memory 1)
  (func (export "low_s") (result i64)
    i32.const 0
    v128.const i32x4 -2 3 -2147483648 2147483647
    v128.const i32x4 7 -5 2 2
    i64x2.extmul_low_i32x4_s
    v128.store
    i32.const 0
    i64.load)
  (func (export "high_s") (result i64)
    i32.const 0
    v128.const i32x4 -2 3 -2147483648 2147483647
    v128.const i32x4 7 -5 2 2
    i64x2.extmul_high_i32x4_s
    v128.store
    i32.const 0
    i64.load offset=8)
  (func (export "low_u") (result i64)
    i32.const 0
    v128.const i32x4 -2 3 4 5
    v128.const i32x4 7 -5 2 2
    i64x2.extmul_low_i32x4_u
    v128.store
    i32.const 0
    i64.load)
  (func (export "high_u") (result i64)
    i32.const 0
    v128.const i32x4 1 2 -2147483648 2147483647
    v128.const i32x4 3 4 2 2
    i64x2.extmul_high_i32x4_u
    v128.store
    i32.const 0
    i64.load))"#;
const EXPORTS: [&str; 4] = ["low_s", "high_s", "low_u", "high_u"];
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
fn i64x2_extmul_matches_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("fixture parse");
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, reference);
    assert_eq!(mini[0], -14);
    assert_eq!(mini[1], i64::from(i32::MAX) * 2);
}
