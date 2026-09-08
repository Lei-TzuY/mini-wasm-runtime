use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "abs_min") (result i32)
    v128.const i8x16 -128 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0
    i8x16.abs
    i8x16.extract_lane_s 0)
  (func (export "neg") (result i32)
    v128.const i8x16 -1 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0
    i8x16.neg
    i8x16.extract_lane_s 0)
  (func (export "popcnt") (result i32)
    v128.const i8x16 -1 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0
    i8x16.popcnt
    i8x16.extract_lane_u 0)
  (func (export "all_true") (result i32)
    v128.const i8x16 1 -1 2 -2 3 -3 4 -4 5 -5 6 -6 7 -7 8 -8
    i8x16.all_true)
  (func (export "not_all_true") (result i32)
    v128.const i8x16 1 -1 2 -2 3 -3 4 0 5 -5 6 -6 7 -7 8 -8
    i8x16.all_true)
  (func (export "bitmask") (result i32)
    v128.const i8x16 -1 0 1 -128 127 0 1 -2 -3 4 5 6 7 8 9 -10
    i8x16.bitmask))
"#;
const EXPORTS: [&str; 6] = [
    "abs_min",
    "neg",
    "popcnt",
    "all_true",
    "not_all_true",
    "bitmask",
];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini must parse i8x16 unary fixture");
    let mut instance =
        MiniInstance::new(module).expect("mini must instantiate i8x16 unary fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            match instance
                .invoke_export_values(export, &[])
                .expect("mini execution must succeed")
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
    let engine = Engine::new(&config).expect("SIMD Wasmtime engine must initialize");
    let module = ReferenceModule::new(&engine, bytes).expect("Wasmtime must compile fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i32>(&mut store, export)
                .expect("export signature")
                .call(&mut store, ())
                .expect("reference execution")
        })
        .collect()
}

#[test]
fn i8x16_unary_reductions_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("fixture must parse");
    let expected = vec![-128, 1, 8, 1, 0, 0x8189];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, expected);
    assert_eq!(reference, expected);
    assert_eq!(mini, reference);
}
