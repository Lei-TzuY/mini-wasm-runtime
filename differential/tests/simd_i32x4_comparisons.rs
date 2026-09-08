use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "eq") (result i32)
    v128.const i32x4 -1 0 5 -2147483648
    v128.const i32x4 0 0 4 2147483647
    i32x4.eq
    i32x4.bitmask)
  (func (export "ne") (result i32)
    v128.const i32x4 -1 0 5 -2147483648
    v128.const i32x4 0 0 4 2147483647
    i32x4.ne
    i32x4.bitmask)
  (func (export "lt_s") (result i32)
    v128.const i32x4 -1 0 5 -2147483648
    v128.const i32x4 0 0 4 2147483647
    i32x4.lt_s
    i32x4.bitmask)
  (func (export "lt_u") (result i32)
    v128.const i32x4 -1 0 5 -2147483648
    v128.const i32x4 0 0 4 2147483647
    i32x4.lt_u
    i32x4.bitmask)
  (func (export "gt_s") (result i32)
    v128.const i32x4 -1 0 5 -2147483648
    v128.const i32x4 0 0 4 2147483647
    i32x4.gt_s
    i32x4.bitmask)
  (func (export "gt_u") (result i32)
    v128.const i32x4 -1 0 5 -2147483648
    v128.const i32x4 0 0 4 2147483647
    i32x4.gt_u
    i32x4.bitmask)
  (func (export "le_s") (result i32)
    v128.const i32x4 -1 0 5 -2147483648
    v128.const i32x4 0 0 4 2147483647
    i32x4.le_s
    i32x4.bitmask)
  (func (export "le_u") (result i32)
    v128.const i32x4 -1 0 5 -2147483648
    v128.const i32x4 0 0 4 2147483647
    i32x4.le_u
    i32x4.bitmask)
  (func (export "ge_s") (result i32)
    v128.const i32x4 -1 0 5 -2147483648
    v128.const i32x4 0 0 4 2147483647
    i32x4.ge_s
    i32x4.bitmask)
  (func (export "ge_u") (result i32)
    v128.const i32x4 -1 0 5 -2147483648
    v128.const i32x4 0 0 4 2147483647
    i32x4.ge_u
    i32x4.bitmask)
)
"#;

const EXPORTS: [&str; 10] = [
    "eq", "ne", "lt_s", "lt_u", "gt_s", "gt_u", "le_s", "le_u", "ge_s", "ge_u",
];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini runtime must parse SIMD comparison fixture");
    let mut instance =
        MiniInstance::new(module).expect("mini runtime must instantiate SIMD comparison fixture");

    EXPORTS
        .into_iter()
        .map(|export| {
            let values = instance
                .invoke_export_values(export, &[])
                .expect("mini SIMD comparison execution must succeed");
            match values.as_slice() {
                [Value::I32(value)] => *value,
                other => panic!("unexpected mini SIMD comparison result for {export}: {other:?}"),
            }
        })
        .collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i32> {
    let mut config = Config::new();
    config.wasm_simd(true);
    let engine = Engine::new(&config).expect("SIMD-enabled Wasmtime engine must initialize");
    let module = ReferenceModule::new(&engine, bytes)
        .expect("Wasmtime must compile SIMD comparison fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate SIMD comparison fixture");

    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i32>(&mut store, export)
                .expect("SIMD comparison export must be [] -> [i32]")
                .call(&mut store, ())
                .expect("Wasmtime SIMD comparison execution must succeed")
        })
        .collect()
}

#[test]
fn i32x4_comparisons_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("SIMD comparison WAT fixture must parse");
    let expected = vec![2, 13, 9, 0, 4, 13, 11, 2, 6, 15];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);

    assert_eq!(mini, expected, "mini SIMD comparison trace drifted");
    assert_eq!(
        reference, expected,
        "Wasmtime SIMD comparison trace drifted"
    );
    assert_eq!(mini, reference, "SIMD comparison traces diverged");
}
