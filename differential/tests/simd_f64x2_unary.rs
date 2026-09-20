use wasm_runtime::{Instance, Value};
use wasmtime::{Engine, Instance as WasmtimeInstance, Module as WasmtimeModule, Store};

const FIXTURE: &str = r#"(module
  (memory 1)
  (func (export "abs") (result i64) i32.const 0 v128.const f64x2 -3.5 -0 f64x2.abs v128.store i32.const 0 i64.load)
  (func (export "neg") (result i64) i32.const 0 v128.const f64x2 3.5 2 f64x2.neg v128.store i32.const 0 i64.load offset=8)
  (func (export "sqrt") (result i64) i32.const 0 v128.const f64x2 4 81 f64x2.sqrt v128.store i32.const 0 i64.load offset=8)
  (func (export "ceil") (result i64) i32.const 0 v128.const f64x2 -1.5 1.5 f64x2.ceil v128.store i32.const 0 i64.load)
  (func (export "floor") (result i64) i32.const 0 v128.const f64x2 -1.5 1.5 f64x2.floor v128.store i32.const 0 i64.load)
  (func (export "trunc") (result i64) i32.const 0 v128.const f64x2 -1.5 1.5 f64x2.trunc v128.store i32.const 0 i64.load)
  (func (export "nearest") (result i64) i32.const 0 v128.const f64x2 -2.5 -0.5 f64x2.nearest v128.store i32.const 0 i64.load))"#;

#[test]
fn f64x2_unary_matches_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("wat");
    let parsed = wasm_parser::parse_module(&bytes).expect("parse");
    let mut mini = Instance::new(parsed).expect("mini");
    let exports = ["abs", "neg", "sqrt", "ceil", "floor", "trunc", "nearest"];
    let mini_values = exports.map(|name| {
        match mini.invoke_export_values(name, &[]).unwrap().as_slice() {
            [Value::I64(v)] => *v,
            _ => panic!(),
        }
    });

    let engine = Engine::default();
    let module = WasmtimeModule::new(&engine, &bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = WasmtimeInstance::new(&mut store, &module, &[]).unwrap();
    let reference = exports.map(|name| {
        instance
            .get_typed_func::<(), i64>(&mut store, name)
            .unwrap()
            .call(&mut store, ())
            .unwrap()
    });

    assert_eq!(mini_values, reference);
    assert_eq!(mini_values[3] as u64, (-1.0f64).to_bits());
    assert_eq!(mini_values[4] as u64, (-2.0f64).to_bits());
    assert_eq!(mini_values[5] as u64, (-1.0f64).to_bits());
    assert_eq!(mini_values[6] as u64, (-2.0f64).to_bits());
}
