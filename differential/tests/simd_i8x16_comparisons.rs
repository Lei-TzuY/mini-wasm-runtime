use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "eq_true") (result i32)
    v128.const i8x16 -128 127 -1 0 1 -128 -2 100 -56 50 0 -1 127 -127 2 2
    v128.const i8x16 127 -128 1 0 2 127 -1 100 100 -56 -1 0 127 -128 3 1
    i8x16.eq
    i8x16.extract_lane_u 3)
  (func (export "eq_false") (result i32)
    v128.const i8x16 -128 127 -1 0 1 -128 -2 100 -56 50 0 -1 127 -127 2 2
    v128.const i8x16 127 -128 1 0 2 127 -1 100 100 -56 -1 0 127 -128 3 1
    i8x16.eq
    i8x16.extract_lane_u 0)
  (func (export "ne_true") (result i32)
    v128.const i8x16 -128 127 -1 0 1 -128 -2 100 -56 50 0 -1 127 -127 2 2
    v128.const i8x16 127 -128 1 0 2 127 -1 100 100 -56 -1 0 127 -128 3 1
    i8x16.ne
    i8x16.extract_lane_u 0)
  (func (export "ne_false") (result i32)
    v128.const i8x16 -128 127 -1 0 1 -128 -2 100 -56 50 0 -1 127 -127 2 2
    v128.const i8x16 127 -128 1 0 2 127 -1 100 100 -56 -1 0 127 -128 3 1
    i8x16.ne
    i8x16.extract_lane_u 3)
  (func (export "lt_s_true") (result i32)
    v128.const i8x16 -128 127 -1 0 1 -128 -2 100 -56 50 0 -1 127 -127 2 2
    v128.const i8x16 127 -128 1 0 2 127 -1 100 100 -56 -1 0 127 -128 3 1
    i8x16.lt_s
    i8x16.extract_lane_u 0)
  (func (export "lt_s_false") (result i32)
    v128.const i8x16 -128 127 -1 0 1 -128 -2 100 -56 50 0 -1 127 -127 2 2
    v128.const i8x16 127 -128 1 0 2 127 -1 100 100 -56 -1 0 127 -128 3 1
    i8x16.lt_s
    i8x16.extract_lane_u 1)
  (func (export "lt_u_true") (result i32)
    v128.const i8x16 -128 127 -1 0 1 -128 -2 100 -56 50 0 -1 127 -127 2 2
    v128.const i8x16 127 -128 1 0 2 127 -1 100 100 -56 -1 0 127 -128 3 1
    i8x16.lt_u
    i8x16.extract_lane_u 1)
  (func (export "lt_u_false") (result i32)
    v128.const i8x16 -128 127 -1 0 1 -128 -2 100 -56 50 0 -1 127 -127 2 2
    v128.const i8x16 127 -128 1 0 2 127 -1 100 100 -56 -1 0 127 -128 3 1
    i8x16.lt_u
    i8x16.extract_lane_u 0)
  (func (export "gt_s_true") (result i32)
    v128.const i8x16 -128 127 -1 0 1 -128 -2 100 -56 50 0 -1 127 -127 2 2
    v128.const i8x16 127 -128 1 0 2 127 -1 100 100 -56 -1 0 127 -128 3 1
    i8x16.gt_s
    i8x16.extract_lane_u 1)
  (func (export "gt_s_false") (result i32)
    v128.const i8x16 -128 127 -1 0 1 -128 -2 100 -56 50 0 -1 127 -127 2 2
    v128.const i8x16 127 -128 1 0 2 127 -1 100 100 -56 -1 0 127 -128 3 1
    i8x16.gt_s
    i8x16.extract_lane_u 0)
  (func (export "gt_u_true") (result i32)
    v128.const i8x16 -128 127 -1 0 1 -128 -2 100 -56 50 0 -1 127 -127 2 2
    v128.const i8x16 127 -128 1 0 2 127 -1 100 100 -56 -1 0 127 -128 3 1
    i8x16.gt_u
    i8x16.extract_lane_u 0)
  (func (export "gt_u_false") (result i32)
    v128.const i8x16 -128 127 -1 0 1 -128 -2 100 -56 50 0 -1 127 -127 2 2
    v128.const i8x16 127 -128 1 0 2 127 -1 100 100 -56 -1 0 127 -128 3 1
    i8x16.gt_u
    i8x16.extract_lane_u 1)
  (func (export "le_s_true") (result i32)
    v128.const i8x16 -128 127 -1 0 1 -128 -2 100 -56 50 0 -1 127 -127 2 2
    v128.const i8x16 127 -128 1 0 2 127 -1 100 100 -56 -1 0 127 -128 3 1
    i8x16.le_s
    i8x16.extract_lane_u 0)
  (func (export "le_s_false") (result i32)
    v128.const i8x16 -128 127 -1 0 1 -128 -2 100 -56 50 0 -1 127 -127 2 2
    v128.const i8x16 127 -128 1 0 2 127 -1 100 100 -56 -1 0 127 -128 3 1
    i8x16.le_s
    i8x16.extract_lane_u 1)
  (func (export "le_u_true") (result i32)
    v128.const i8x16 -128 127 -1 0 1 -128 -2 100 -56 50 0 -1 127 -127 2 2
    v128.const i8x16 127 -128 1 0 2 127 -1 100 100 -56 -1 0 127 -128 3 1
    i8x16.le_u
    i8x16.extract_lane_u 1)
  (func (export "le_u_false") (result i32)
    v128.const i8x16 -128 127 -1 0 1 -128 -2 100 -56 50 0 -1 127 -127 2 2
    v128.const i8x16 127 -128 1 0 2 127 -1 100 100 -56 -1 0 127 -128 3 1
    i8x16.le_u
    i8x16.extract_lane_u 0)
  (func (export "ge_s_true") (result i32)
    v128.const i8x16 -128 127 -1 0 1 -128 -2 100 -56 50 0 -1 127 -127 2 2
    v128.const i8x16 127 -128 1 0 2 127 -1 100 100 -56 -1 0 127 -128 3 1
    i8x16.ge_s
    i8x16.extract_lane_u 1)
  (func (export "ge_s_false") (result i32)
    v128.const i8x16 -128 127 -1 0 1 -128 -2 100 -56 50 0 -1 127 -127 2 2
    v128.const i8x16 127 -128 1 0 2 127 -1 100 100 -56 -1 0 127 -128 3 1
    i8x16.ge_s
    i8x16.extract_lane_u 0)
  (func (export "ge_u_true") (result i32)
    v128.const i8x16 -128 127 -1 0 1 -128 -2 100 -56 50 0 -1 127 -127 2 2
    v128.const i8x16 127 -128 1 0 2 127 -1 100 100 -56 -1 0 127 -128 3 1
    i8x16.ge_u
    i8x16.extract_lane_u 0)
  (func (export "ge_u_false") (result i32)
    v128.const i8x16 -128 127 -1 0 1 -128 -2 100 -56 50 0 -1 127 -127 2 2
    v128.const i8x16 127 -128 1 0 2 127 -1 100 100 -56 -1 0 127 -128 3 1
    i8x16.ge_u
    i8x16.extract_lane_u 1)
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
    let module = parse_module(bytes).expect("mini runtime must parse i8x16 comparison fixture");
    let mut instance =
        MiniInstance::new(module).expect("mini runtime must instantiate i8x16 comparison fixture");

    EXPORTS
        .into_iter()
        .map(|export| {
            let values = instance
                .invoke_export_values(export, &[])
                .expect("mini i8x16 comparison execution must succeed");
            match values.as_slice() {
                [Value::I32(value)] => *value,
                other => panic!("unexpected mini i8x16 comparison result for {export}: {other:?}"),
            }
        })
        .collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i32> {
    let mut config = Config::new();
    config.wasm_simd(true);
    let engine = Engine::new(&config).expect("SIMD-enabled Wasmtime engine must initialize");
    let module = ReferenceModule::new(&engine, bytes)
        .expect("Wasmtime must compile i8x16 comparison fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate i8x16 comparison fixture");

    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i32>(&mut store, export)
                .expect("i8x16 comparison export must be [] -> [i32]")
                .call(&mut store, ())
                .expect("Wasmtime i8x16 comparison execution must succeed")
        })
        .collect()
}

#[test]
fn i8x16_comparisons_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("i8x16 comparison WAT fixture must parse");
    let expected = vec![
        255, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255, 0,
    ];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);

    assert_eq!(mini, expected, "mini i8x16 comparison trace drifted");
    assert_eq!(
        reference, expected,
        "Wasmtime i8x16 comparison trace drifted"
    );
    assert_eq!(mini, reference, "i8x16 comparison traces diverged");
}
