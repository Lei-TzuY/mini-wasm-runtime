use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"(module
  (func (export "abs_min") (result i64)
    v128.const i64x2 -9223372036854775808 -7
    i64x2.abs
    i64x2.extract_lane 0)
  (func (export "abs_lane1") (result i64)
    v128.const i64x2 -9223372036854775808 -7
    i64x2.abs
    i64x2.extract_lane 1)
  (func (export "neg_min") (result i64)
    v128.const i64x2 -9223372036854775808 -7
    i64x2.neg
    i64x2.extract_lane 0)
  (func (export "neg_lane1") (result i64)
    v128.const i64x2 -9223372036854775808 -7
    i64x2.neg
    i64x2.extract_lane 1)
  (func (export "all_true") (result i32)
    v128.const i64x2 1 -2
    i64x2.all_true)
  (func (export "has_zero") (result i32)
    v128.const i64x2 1 0
    i64x2.all_true)
  (func (export "bitmask_one") (result i32)
    v128.const i64x2 -1 9223372036854775807
    i64x2.bitmask)
  (func (export "bitmask_both") (result i32)
    v128.const i64x2 -1 -9223372036854775808
    i64x2.bitmask))"#;

const I64_EXPORTS: [(&str, i64); 4] = [
    ("abs_min", i64::MIN),
    ("abs_lane1", 7),
    ("neg_min", i64::MIN),
    ("neg_lane1", 7),
];

const I32_EXPORTS: [(&str, i32); 4] = [
    ("all_true", 1),
    ("has_zero", 0),
    ("bitmask_one", 1),
    ("bitmask_both", 3),
];

#[test]
fn i64x2_unary_reductions_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("i64x2 WAT parses");
    let module = parse_module(&bytes).expect("mini parses i64x2 fixture");
    let mut mini = MiniInstance::new(module).expect("mini instantiates i64x2 fixture");

    let mut config = Config::new();
    config.wasm_simd(true);
    let engine = Engine::new(&config).expect("SIMD engine");
    let module = ReferenceModule::new(&engine, &bytes).expect("Wasmtime compiles i64x2 fixture");
    let mut store = Store::new(&engine, ());
    let reference =
        ReferenceInstance::new(&mut store, &module, &[]).expect("Wasmtime instantiates");

    for (name, expected) in I64_EXPORTS {
        let mini_value = match mini
            .invoke_export_values(name, &[])
            .expect("mini i64x2 unary execution")
            .as_slice()
        {
            [Value::I64(value)] => *value,
            other => panic!("unexpected mini i64 result for {name}: {other:?}"),
        };
        let reference_value = reference
            .get_typed_func::<(), i64>(&mut store, name)
            .expect("i64x2 unary signature")
            .call(&mut store, ())
            .expect("Wasmtime i64x2 unary execution");
        assert_eq!(mini_value, expected, "mini {name} drifted");
        assert_eq!(reference_value, expected, "Wasmtime {name} drifted");
        assert_eq!(mini_value, reference_value, "{name} differential mismatch");
    }

    for (name, expected) in I32_EXPORTS {
        let mini_value = match mini
            .invoke_export_values(name, &[])
            .expect("mini i64x2 reduction execution")
            .as_slice()
        {
            [Value::I32(value)] => *value,
            other => panic!("unexpected mini i32 result for {name}: {other:?}"),
        };
        let reference_value = reference
            .get_typed_func::<(), i32>(&mut store, name)
            .expect("i64x2 reduction signature")
            .call(&mut store, ())
            .expect("Wasmtime i64x2 reduction execution");
        assert_eq!(mini_value, expected, "mini {name} drifted");
        assert_eq!(reference_value, expected, "Wasmtime {name} drifted");
        assert_eq!(mini_value, reference_value, "{name} differential mismatch");
    }
}
