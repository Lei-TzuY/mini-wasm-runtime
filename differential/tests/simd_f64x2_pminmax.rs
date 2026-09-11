use wasm_runtime::{Instance, Value};
use wasmtime::{Engine, Instance as WasmtimeInstance, Module as WasmtimeModule, Store};

const FIXTURE: &str = r#"(module
 (memory 1)
 (func (export "pmin") (result i64) i32.const 0 v128.const f64x2 3 -2 v128.const f64x2 4 -5 f64x2.pmin v128.store i32.const 0 i64.load offset=8)
 (func (export "pmax") (result i64) i32.const 0 v128.const f64x2 3 -2 v128.const f64x2 4 -5 f64x2.pmax v128.store i32.const 0 i64.load))"#;

#[test]
fn f64x2_pmin_pmax_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("wat");
    let parsed = wasm_parser::parse_module(&bytes).expect("parse");
    let mut mini = Instance::new(parsed).expect("mini");
    let mini_pmin = match mini.invoke_export_values("pmin", &[]).unwrap().as_slice() {
        [Value::I64(v)] => *v,
        _ => panic!(),
    };
    let mini_pmax = match mini.invoke_export_values("pmax", &[]).unwrap().as_slice() {
        [Value::I64(v)] => *v,
        _ => panic!(),
    };

    let engine = Engine::default();
    let module = WasmtimeModule::new(&engine, &bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = WasmtimeInstance::new(&mut store, &module, &[]).unwrap();
    let ref_pmin = instance
        .get_typed_func::<(), i64>(&mut store, "pmin")
        .unwrap()
        .call(&mut store, ())
        .unwrap();
    let ref_pmax = instance
        .get_typed_func::<(), i64>(&mut store, "pmax")
        .unwrap()
        .call(&mut store, ())
        .unwrap();

    assert_eq!((mini_pmin, mini_pmax), (ref_pmin, ref_pmax));
}
