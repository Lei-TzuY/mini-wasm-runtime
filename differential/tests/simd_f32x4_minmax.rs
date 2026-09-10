use wasm_runtime::{Instance, Value};
use wasmtime::{Engine, Instance as WasmtimeInstance, Module as WasmtimeModule, Store};

const FIXTURE: &str = r#"(module
 (memory 1)
 (func (export "min") (result i32) i32.const 0 v128.const f32x4 3 -2 8 1 v128.const f32x4 4 -5 7 2 f32x4.min v128.store i32.const 0 i32.load)
 (func (export "max") (result i32) i32.const 0 v128.const f32x4 3 -2 8 1 v128.const f32x4 4 -5 7 2 f32x4.max v128.store i32.const 0 i32.load offset=8))"#;

#[test]
fn f32x4_minmax_matches_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("wat");
    let parsed = wasm_parser::parse_module(&bytes).expect("parse");
    let mut mini = Instance::new(parsed).expect("mini");
    let mini_min = match mini
        .invoke_export_values("min", &[])
        .unwrap()
        .as_slice()
    {
        [Value::I32(v)] => *v,
        _ => panic!(),
    };
    let mini_max = match mini
        .invoke_export_values("max", &[])
        .unwrap()
        .as_slice()
    {
        [Value::I32(v)] => *v,
        _ => panic!(),
    };

    let engine = Engine::default();
    let module = WasmtimeModule::new(&engine, &bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = WasmtimeInstance::new(&mut store, &module, &[]).unwrap();
    let ref_min = instance
        .get_typed_func::<(), i32>(&mut store, "min")
        .unwrap()
        .call(&mut store, ())
        .unwrap();
    let ref_max = instance
        .get_typed_func::<(), i32>(&mut store, "max")
        .unwrap()
        .call(&mut store, ())
        .unwrap();

    assert_eq!((mini_min, mini_max), (ref_min, ref_max));
}
