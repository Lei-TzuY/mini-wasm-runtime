use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "not") (result i32)
    v128.const i32x4 252645135 252645135 252645135 252645135
    v128.not
    i32x4.extract_lane 0)
  (func (export "and") (result i32)
    v128.const i32x4 252645135 252645135 252645135 252645135
    v128.const i32x4 858993459 858993459 858993459 858993459
    v128.and
    i32x4.extract_lane 0)
  (func (export "andnot") (result i32)
    v128.const i32x4 252645135 252645135 252645135 252645135
    v128.const i32x4 858993459 858993459 858993459 858993459
    v128.andnot
    i32x4.extract_lane 0)
  (func (export "or") (result i32)
    v128.const i32x4 252645135 252645135 252645135 252645135
    v128.const i32x4 858993459 858993459 858993459 858993459
    v128.or
    i32x4.extract_lane 0)
  (func (export "xor") (result i32)
    v128.const i32x4 252645135 252645135 252645135 252645135
    v128.const i32x4 858993459 858993459 858993459 858993459
    v128.xor
    i32x4.extract_lane 0)
  (func (export "bitselect") (result i32)
    v128.const i32x4 -1431655766 -1431655766 -1431655766 -1431655766
    v128.const i32x4 1431655765 1431655765 1431655765 1431655765
    v128.const i32x4 -252645136 -252645136 -252645136 -252645136
    v128.bitselect
    i32x4.extract_lane 0)
  (func (export "any_zero") (result i32)
    v128.const i32x4 0 0 0 0
    v128.any_true)
  (func (export "any_nonzero") (result i32)
    v128.const i32x4 0 0 0 1
    v128.any_true))
"#;

const EXPORTS: [&str; 8] = [
    "not",
    "and",
    "andnot",
    "or",
    "xor",
    "bitselect",
    "any_zero",
    "any_nonzero",
];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini runtime must parse v128 bitwise fixture");
    let mut instance = MiniInstance::new(module).expect("mini runtime must instantiate fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            let values = instance
                .invoke_export_values(export, &[])
                .expect("mini v128 bitwise execution must succeed");
            match values.as_slice() {
                [Value::I32(value)] => *value,
                other => panic!("unexpected mini v128 bitwise result for {export}: {other:?}"),
            }
        })
        .collect()
}

fn reference_engine() -> Engine {
    let mut config = Config::new();
    config.wasm_simd(true);
    Engine::new(&config).expect("SIMD-enabled Wasmtime engine must initialize")
}

fn reference_trace(bytes: &[u8]) -> Vec<i32> {
    let engine = reference_engine();
    let module = ReferenceModule::new(&engine, bytes).expect("Wasmtime must compile fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i32>(&mut store, export)
                .expect("v128 bitwise export must be [] -> [i32]")
                .call(&mut store, ())
                .expect("Wasmtime v128 bitwise execution must succeed")
        })
        .collect()
}

#[test]
fn v128_bitwise_mask_semantics_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("v128 bitwise WAT fixture must parse");
    let expected = vec![
        -252645136,
        50529027,
        202116108,
        1061109567,
        1010580540,
        -1515870811,
        0,
        1,
    ];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, expected, "mini v128 bitwise trace drifted");
    assert_eq!(reference, expected, "Wasmtime v128 bitwise trace drifted");
    assert_eq!(mini, reference, "v128 bitwise traces diverged");
}
