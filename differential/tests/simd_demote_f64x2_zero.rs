use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"(module
  (func (export "lane0") (result f32) v128.const f64x2 1.5 -2.25 f32x4.demote_f64x2_zero f32x4.extract_lane 0)
  (func (export "lane1") (result f32) v128.const f64x2 1.5 -2.25 f32x4.demote_f64x2_zero f32x4.extract_lane 1)
  (func (export "lane2") (result f32) v128.const f64x2 1.5 -2.25 f32x4.demote_f64x2_zero f32x4.extract_lane 2)
  (func (export "lane3") (result f32) v128.const f64x2 1.5 -2.25 f32x4.demote_f64x2_zero f32x4.extract_lane 3))"#;

#[test]
fn demote_f64x2_zero_matches_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("WAT parses");
    let module = parse_module(&bytes).expect("mini parses");
    let mut mini = MiniInstance::new(module).expect("mini instantiates");
    let mut cfg = Config::new();
    cfg.wasm_simd(true);
    let engine = Engine::new(&cfg).unwrap();
    let module = ReferenceModule::new(&engine, &bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let reference = ReferenceInstance::new(&mut store, &module, &[]).unwrap();
    for name in ["lane0", "lane1", "lane2", "lane3"] {
        let mini_value = match mini.invoke_export_values(name, &[]).unwrap().as_slice() {
            [Value::F32(v)] => *v,
            other => panic!("unexpected mini result: {other:?}"),
        };
        let reference_value = reference
            .get_typed_func::<(), f32>(&mut store, name)
            .unwrap()
            .call(&mut store, ())
            .unwrap();
        assert_eq!(mini_value.to_bits(), reference_value.to_bits(), "{name}");
    }
}
