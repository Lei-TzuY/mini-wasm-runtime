use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"(module
  (func (export "run") (result i32)
    v128.const i8x16 2 -3 4 5 0 0 0 0 0 0 0 0 0 0 0 0
    v128.const i8x16 4 5 6 7 0 0 0 0 0 0 0 0 0 0 0 0
    v128.const i32x4 10 0 0 0
    i32x4.relaxed_dot_i8x16_i7x16_add_s
    i32x4.extract_lane 0))"#;

#[test]
fn relaxed_dot_add_matches_wasmtime_for_defined_lanes() {
    let bytes = wat::parse_str(FIXTURE).unwrap();
    let parsed = parse_module(&bytes).unwrap();
    let mut mini = MiniInstance::new(parsed).unwrap();
    let got = match mini.invoke_export("run", &[]).unwrap().as_slice() {
        [Value::I32(v)] => *v,
        _ => panic!(),
    };
    let mut cfg = Config::new();
    cfg.wasm_relaxed_simd(true);
    let engine = Engine::new(&cfg).unwrap();
    let module = ReferenceModule::new(&engine, &bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let inst = ReferenceInstance::new(&mut store, &module, &[]).unwrap();
    let expected = inst
        .get_typed_func::<(), i32>(&mut store, "run")
        .unwrap()
        .call(&mut store, ())
        .unwrap();
    assert_eq!(got, expected);
}
