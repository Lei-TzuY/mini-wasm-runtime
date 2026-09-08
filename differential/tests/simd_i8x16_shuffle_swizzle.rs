use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "shuffle_cross") (result i32)
    v128.const i8x16 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15
    v128.const i8x16 100 101 102 103 104 105 106 107 108 109 110 111 112 113 114 115
    i8x16.shuffle 0 17 2 19 4 21 6 23 8 25 10 27 12 29 14 31
    i8x16.extract_lane_u 1)
  (func (export "shuffle_repeat") (result i32)
    v128.const i8x16 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15
    v128.const i8x16 100 101 102 103 104 105 106 107 108 109 110 111 112 113 114 115
    i8x16.shuffle 31 31 31 31 31 31 31 31 31 31 31 31 31 31 31 31
    i8x16.extract_lane_u 7)
  (func (export "swizzle_in_range") (result i32)
    v128.const i8x16 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25
    v128.const i8x16 15 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14
    i8x16.swizzle
    i8x16.extract_lane_u 0)
  (func (export "swizzle_oob") (result i32)
    v128.const i8x16 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25
    v128.const i8x16 16 -1 0 1 2 3 4 5 6 7 8 9 10 11 12 13
    i8x16.swizzle
    i8x16.extract_lane_u 0))
"#;

const EXPORTS: [&str; 4] = [
    "shuffle_cross",
    "shuffle_repeat",
    "swizzle_in_range",
    "swizzle_oob",
];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini runtime must parse shuffle/swizzle fixture");
    let mut instance =
        MiniInstance::new(module).expect("mini runtime must instantiate shuffle/swizzle fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            let values = instance
                .invoke_export_values(export, &[])
                .expect("mini shuffle/swizzle execution must succeed");
            match values.as_slice() {
                [Value::I32(value)] => *value,
                other => panic!("unexpected mini result for {export}: {other:?}"),
            }
        })
        .collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i32> {
    let mut config = Config::new();
    config.wasm_simd(true);
    let engine = Engine::new(&config).expect("SIMD-enabled Wasmtime engine must initialize");
    let module = ReferenceModule::new(&engine, bytes)
        .expect("Wasmtime must compile shuffle/swizzle fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate shuffle/swizzle fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i32>(&mut store, export)
                .expect("shuffle/swizzle export must be [] -> [i32]")
                .call(&mut store, ())
                .expect("Wasmtime shuffle/swizzle execution must succeed")
        })
        .collect()
}

#[test]
fn i8x16_shuffle_and_swizzle_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("shuffle/swizzle WAT fixture must parse");
    let expected = vec![101, 115, 25, 0];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, expected, "mini shuffle/swizzle trace drifted");
    assert_eq!(
        reference, expected,
        "Wasmtime shuffle/swizzle trace drifted"
    );
    assert_eq!(mini, reference, "shuffle/swizzle traces diverged");
}
