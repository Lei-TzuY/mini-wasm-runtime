use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};
const FIXTURE: &str = r#"(module (func (export "run") (result i32) v128.const i16x8 16384 0 0 0 0 0 0 0 v128.const i16x8 16384 0 0 0 0 0 0 0 i16x8.relaxed_q15mulr_s i16x8.extract_lane_s 0))"#;
#[test]
fn relaxed_q15mulr_matches_wasmtime_for_defined_lane() {
    let bytes = wat::parse_str(FIXTURE).unwrap();
    let parsed = parse_module(&bytes).unwrap();
    let mut mini = MiniInstance::new(parsed).unwrap();
    let mv = match mini.invoke_export_values("run", &[]).unwrap().as_slice() {
        [Value::I32(v)] => *v,
        _ => panic!(),
    };
    let mut c = Config::new();
    c.wasm_simd(true);
    c.wasm_relaxed_simd(true);
    let e = Engine::new(&c).unwrap();
    let m = ReferenceModule::new(&e, &bytes).unwrap();
    let mut s = Store::new(&e, ());
    let x = ReferenceInstance::new(&mut s, &m, &[]).unwrap();
    let rv = x
        .get_typed_func::<(), i32>(&mut s, "run")
        .unwrap()
        .call(&mut s, ())
        .unwrap();
    assert_eq!(mv, 8192);
    assert_eq!(mv, rv);
}
