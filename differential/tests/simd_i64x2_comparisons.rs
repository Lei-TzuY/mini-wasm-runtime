use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (memory 1)
  (func (export "eq") (result i64)
    i32.const 0
    v128.const i64x2 7 -4
    v128.const i64x2 7 3
    i64x2.eq
    v128.store
    i32.const 0
    i64.load)
  (func (export "ne") (result i64)
    i32.const 0
    v128.const i64x2 7 -4
    v128.const i64x2 7 3
    i64x2.ne
    v128.store
    i32.const 0
    i64.load offset=8)
  (func (export "lt") (result i64)
    i32.const 0
    v128.const i64x2 -9 8
    v128.const i64x2 2 8
    i64x2.lt_s
    v128.store
    i32.const 0
    i64.load)
  (func (export "gt") (result i64)
    i32.const 0
    v128.const i64x2 -9 8
    v128.const i64x2 2 3
    i64x2.gt_s
    v128.store
    i32.const 0
    i64.load offset=8)
  (func (export "le") (result i64)
    i32.const 0
    v128.const i64x2 5 9
    v128.const i64x2 5 7
    i64x2.le_s
    v128.store
    i32.const 0
    i64.load)
  (func (export "ge") (result i64)
    i32.const 0
    v128.const i64x2 5 -1
    v128.const i64x2 5 -1
    i64x2.ge_s
    v128.store
    i32.const 0
    i64.load offset=8))
"#;

const EXPORTS: [&str; 6] = ["eq", "ne", "lt", "gt", "le", "ge"];

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
fn i64x2_comparisons_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("fixture parse");
    let expected = vec![-1, -1, -1, -1, -1, -1];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, expected);
    assert_eq!(reference, expected);
    assert_eq!(mini, reference);
}
