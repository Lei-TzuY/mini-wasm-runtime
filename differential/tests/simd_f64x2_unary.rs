use wasm_runtime::{Instance, Value};
use wasmtime::{Engine, Instance as WasmtimeInstance, Module as WasmtimeModule, Store};

const FIXTURE: &str = r#"(module
  (memory 1)
  (func (export "abs") (result i64) i32.const 0 v128.const f64x2 -3.5 -0 f64x2.abs v128.store i32.const 0 i64.load)
  (func (export "neg") (result i64) i32.const 0 v128.const f64x2 3.5 2 f64x2.neg v128.store i32.const 0 i64.load offset=8)
  (func (export "sqrt") (result i64) i32.const 0 v128.const f64x2 4 81 f64x2.sqrt v128.store i32.const 0 i64.load offset=8))"#;

#[test]
fn f64x2_unary_matches_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("wat");
    let parsed = wasm_parser::parse_module(&bytes).expect("parse");
    let mut mini = Instance::new(parsed).expect("mini");
    let mini_values = ["abs", "neg", "sqrt"].map(|name| {
        match mini.invoke_export_values(name, &[]).unwrap().as_slice() {
            [Value::I64(v)] => *v,
            _ => panic!(),
        }
    });

    let engine = Engine::default();
    let module = WasmtimeModule::new(&engine, &bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = WasmtimeInstance::new(&mut store, &module, &[]).unwrap();
    let reference = ["abs", "neg", "sqrt"].map(|name| {
        instance
            .get_typed_func::<(), i64>(&mut store, name)
            .unwrap()
            .call(&mut store, ())
            .unwrap()
    });

    assert_eq!(mini_values, reference);
}
