use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};
const FIXTURE: &str = r#"
(module
  (memory 1)
  (func (export "shl") (result i64)
    i32.const 0 v128.const i64x2 4611686018427387904 4611686018427387904 i32.const 65 i64x2.shl v128.store
    i32.const 0 i64.load)
  (func (export "shr_s") (result i64)
    i32.const 0 v128.const i64x2 -2 -2 i32.const 1 i64x2.shr_s v128.store
    i32.const 0 i64.load)
  (func (export "shr_u") (result i64)
    i32.const 0 v128.const i64x2 -9223372036854775808 -9223372036854775808 i32.const 1 i64x2.shr_u v128.store
    i32.const 0 i64.load))
"#;
const EXPORTS: [&str; 3] = ["shl", "shr_s", "shr_u"];
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
fn i64x2_shifts_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("fixture parse");
    let expected = vec![i64::MIN, -1, 0x4000_0000_0000_0000];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, expected);
    assert_eq!(reference, expected);
    assert_eq!(mini, reference);
}
