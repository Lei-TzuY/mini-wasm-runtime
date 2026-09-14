use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const WAT: &str = r#"(module
  (func (export "i32_replace") (result i32) i32.const 7 i32x4.splat i32.const 42 i32x4.replace_lane 2 i32x4.extract_lane 2)
  (func (export "i64_lane") (result i64) i64.const 7 i64x2.splat i64.const 42 i64x2.replace_lane 1 i64x2.extract_lane 1)
  (func (export "f32_lane") (result f32) f32.const 1.5 f32x4.splat f32.const -2.25 f32x4.replace_lane 3 f32x4.extract_lane 3)
  (func (export "f64_lane") (result f64) f64.const 1.5 f64x2.splat f64.const -2.25 f64x2.replace_lane 1 f64x2.extract_lane 1)
)"#;

#[test]
fn remaining_lane_primitives_match_wasmtime() {
    let bytes = wat::parse_str(WAT).unwrap();
    let module = parse_module(&bytes).unwrap();
    let mut mini = MiniInstance::new(module).unwrap();
    let mut config = Config::new();
    config.wasm_simd(true);
    let engine = Engine::new(&config).unwrap();
    let rm = ReferenceModule::new(&engine, &bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let r = ReferenceInstance::new(&mut store, &rm, &[]).unwrap();
    assert_eq!(
        mini.invoke_export_values("i32_replace", &[]).unwrap(),
        vec![Value::I32(
            r.get_typed_func::<(), i32>(&mut store, "i32_replace")
                .unwrap()
                .call(&mut store, ())
                .unwrap()
        )]
    );
    assert_eq!(
        mini.invoke_export_values("i64_lane", &[]).unwrap(),
        vec![Value::I64(
            r.get_typed_func::<(), i64>(&mut store, "i64_lane")
                .unwrap()
                .call(&mut store, ())
                .unwrap()
        )]
    );
    let mf = mini.invoke_export_values("f32_lane", &[]).unwrap();
    let rf = r
        .get_typed_func::<(), f32>(&mut store, "f32_lane")
        .unwrap()
        .call(&mut store, ())
        .unwrap();
    match mf.as_slice() {
        [Value::F32(v)] => assert_eq!(v.to_bits(), rf.to_bits()),
        x => panic!("{x:?}"),
    }
    let mf = mini.invoke_export_values("f64_lane", &[]).unwrap();
    let rf = r
        .get_typed_func::<(), f64>(&mut store, "f64_lane")
        .unwrap()
        .call(&mut store, ())
        .unwrap();
    match mf.as_slice() {
        [Value::F64(v)] => assert_eq!(v.to_bits(), rf.to_bits()),
        x => panic!("{x:?}"),
    }
}
