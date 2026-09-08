use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "eq_true") (result i32)
    v128.const i16x8 -32768 32767 -1 0 1 -32768 -2 100
    v128.const i16x8 32767 -32768 1 0 2 32767 -1 100
    i16x8.eq
    i16x8.extract_lane_u 3)
  (func (export "eq_false") (result i32)
    v128.const i16x8 -32768 32767 -1 0 1 -32768 -2 100
    v128.const i16x8 32767 -32768 1 0 2 32767 -1 100
    i16x8.eq
    i16x8.extract_lane_u 0)
  (func (export "ne_true") (result i32)
    v128.const i16x8 -32768 32767 -1 0 1 -32768 -2 100
    v128.const i16x8 32767 -32768 1 0 2 32767 -1 100
    i16x8.ne
    i16x8.extract_lane_u 0)
  (func (export "ne_false") (result i32)
    v128.const i16x8 -32768 32767 -1 0 1 -32768 -2 100
    v128.const i16x8 32767 -32768 1 0 2 32767 -1 100
    i16x8.ne
    i16x8.extract_lane_u 3)
  (func (export "lt_s_true") (result i32)
    v128.const i16x8 -32768 32767 -1 0 1 -32768 -2 100
    v128.const i16x8 32767 -32768 1 0 2 32767 -1 100
    i16x8.lt_s
    i16x8.extract_lane_u 0)
  (func (export "lt_s_false") (result i32)
    v128.const i16x8 -32768 32767 -1 0 1 -32768 -2 100
    v128.const i16x8 32767 -32768 1 0 2 32767 -1 100
    i16x8.lt_s
    i16x8.extract_lane_u 1)
  (func (export "lt_u_true") (result i32)
    v128.const i16x8 -32768 32767 -1 0 1 -32768 -2 100
    v128.const i16x8 32767 -32768 1 0 2 32767 -1 100
    i16x8.lt_u
    i16x8.extract_lane_u 1)
  (func (export "lt_u_false") (result i32)
    v128.const i16x8 -32768 32767 -1 0 1 -32768 -2 100
    v128.const i16x8 32767 -32768 1 0 2 32767 -1 100
    i16x8.lt_u
    i16x8.extract_lane_u 0)
  (func (export "gt_s_true") (result i32)
    v128.const i16x8 -32768 32767 -1 0 1 -32768 -2 100
    v128.const i16x8 32767 -32768 1 0 2 32767 -1 100
    i16x8.gt_s
    i16x8.extract_lane_u 1)
  (func (export "gt_s_false") (result i32)
    v128.const i16x8 -32768 32767 -1 0 1 -32768 -2 100
    v128.const i16x8 32767 -32768 1 0 2 32767 -1 100
    i16x8.gt_s
    i16x8.extract_lane_u 0)
  (func (export "gt_u_true") (result i32)
    v128.const i16x8 -32768 32767 -1 0 1 -32768 -2 100
    v128.const i16x8 32767 -32768 1 0 2 32767 -1 100
    i16x8.gt_u
    i16x8.extract_lane_u 0)
  (func (export "gt_u_false") (result i32)
    v128.const i16x8 -32768 32767 -1 0 1 -32768 -2 100
    v128.const i16x8 32767 -32768 1 0 2 32767 -1 100
    i16x8.gt_u
    i16x8.extract_lane_u 1)
  (func (export "le_s_true") (result i32)
    v128.const i16x8 -32768 32767 -1 0 1 -32768 -2 100
    v128.const i16x8 32767 -32768 1 0 2 32767 -1 100
    i16x8.le_s
    i16x8.extract_lane_u 0)
  (func (export "le_s_false") (result i32)
    v128.const i16x8 -32768 32767 -1 0 1 -32768 -2 100
    v128.const i16x8 32767 -32768 1 0 2 32767 -1 100
    i16x8.le_s
    i16x8.extract_lane_u 1)
  (func (export "le_u_true") (result i32)
    v128.const i16x8 -32768 32767 -1 0 1 -32768 -2 100
    v128.const i16x8 32767 -32768 1 0 2 32767 -1 100
    i16x8.le_u
    i16x8.extract_lane_u 1)
  (func (export "le_u_false") (result i32)
    v128.const i16x8 -32768 32767 -1 0 1 -32768 -2 100
    v128.const i16x8 32767 -32768 1 0 2 32767 -1 100
    i16x8.le_u
    i16x8.extract_lane_u 0)
  (func (export "ge_s_true") (result i32)
    v128.const i16x8 -32768 32767 -1 0 1 -32768 -2 100
    v128.const i16x8 32767 -32768 1 0 2 32767 -1 100
    i16x8.ge_s
    i16x8.extract_lane_u 1)
  (func (export "ge_s_false") (result i32)
    v128.const i16x8 -32768 32767 -1 0 1 -32768 -2 100
    v128.const i16x8 32767 -32768 1 0 2 32767 -1 100
    i16x8.ge_s
    i16x8.extract_lane_u 0)
  (func (export "ge_u_true") (result i32)
    v128.const i16x8 -32768 32767 -1 0 1 -32768 -2 100
    v128.const i16x8 32767 -32768 1 0 2 32767 -1 100
    i16x8.ge_u
    i16x8.extract_lane_u 0)
  (func (export "ge_u_false") (result i32)
    v128.const i16x8 -32768 32767 -1 0 1 -32768 -2 100
    v128.const i16x8 32767 -32768 1 0 2 32767 -1 100
    i16x8.ge_u
    i16x8.extract_lane_u 1)
)
"#;

const EXPORTS: [&str; 20] = [
    "eq_true",
    "eq_false",
    "ne_true",
    "ne_false",
    "lt_s_true",
    "lt_s_false",
    "lt_u_true",
    "lt_u_false",
    "gt_s_true",
    "gt_s_false",
    "gt_u_true",
    "gt_u_false",
    "le_s_true",
    "le_s_false",
    "le_u_true",
    "le_u_false",
    "ge_s_true",
    "ge_s_false",
    "ge_u_true",
    "ge_u_false",
];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini runtime must parse i16x8 comparison fixture");
    let mut instance =
        MiniInstance::new(module).expect("mini runtime must instantiate i16x8 comparison fixture");

    EXPORTS
        .into_iter()
        .map(|export| {
            let values = instance
                .invoke_export_values(export, &[])
                .expect("mini i16x8 comparison execution must succeed");
            match values.as_slice() {
                [Value::I32(value)] => *value,
                other => panic!("unexpected mini i16x8 comparison result for {export}: {other:?}"),
            }
        })
        .collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i32> {
    let mut config = Config::new();
    config.wasm_simd(true);
    let engine = Engine::new(&config).expect("SIMD-enabled Wasmtime engine must initialize");
    let module = ReferenceModule::new(&engine, bytes)
        .expect("Wasmtime must compile i16x8 comparison fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate i16x8 comparison fixture");

    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i32>(&mut store, export)
                .expect("i16x8 comparison export must be [] -> [i32]")
                .call(&mut store, ())
                .expect("Wasmtime i16x8 comparison execution must succeed")
        })
        .collect()
}

#[test]
fn i16x8_comparisons_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("i16x8 comparison WAT fixture must parse");
    let expected = vec![
        65_535, 0, 65_535, 0, 65_535, 0, 65_535, 0, 65_535, 0, 65_535, 0, 65_535, 0, 65_535, 0,
        65_535, 0, 65_535, 0,
    ];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);

    assert_eq!(mini, expected, "mini i16x8 comparison trace drifted");
    assert_eq!(
        reference, expected,
        "Wasmtime i16x8 comparison trace drifted"
    );
    assert_eq!(mini, reference, "i16x8 comparison traces diverged");
}
