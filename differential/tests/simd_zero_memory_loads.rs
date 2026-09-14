use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"(module
  (memory 1) (data (i32.const 0) "\01\02\03\04\05\06\07\08")
  (func (export "l32") (result i32) i32.const 0 v128.load32_zero i32x4.extract_lane 0)
  (func (export "z32") (result i32) i32.const 0 v128.load32_zero i32x4.extract_lane 1)
  (func (export "l64") (result i64) i32.const 0 v128.load64_zero i64x2.extract_lane 0)
  (func (export "z64") (result i64) i32.const 0 v128.load64_zero i64x2.extract_lane 1))"#;

#[test]
fn simd_zero_loads_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("WAT parses");
    let module = parse_module(&bytes).expect("mini parses");
    let mut mini = MiniInstance::new(module).expect("mini instantiates");
    let mini_i32 = |instance: &mut MiniInstance, name: &str| match instance
        .invoke_export_values(name, &[])
        .unwrap()
        .as_slice()
    {
        [Value::I32(v)] => *v,
        other => panic!("unexpected {other:?}"),
    };
    let mini_i64 = |instance: &mut MiniInstance, name: &str| match instance
        .invoke_export_values(name, &[])
        .unwrap()
        .as_slice()
    {
        [Value::I64(v)] => *v,
        other => panic!("unexpected {other:?}"),
    };
    let expected32 = mini_i32(&mut mini, "l32");
    assert_eq!(expected32, 0x04030201);
    assert_eq!(mini_i32(&mut mini, "z32"), 0);
    let expected64 = mini_i64(&mut mini, "l64");
    assert_eq!(expected64, 0x0807060504030201);
    assert_eq!(mini_i64(&mut mini, "z64"), 0);
    let mut cfg = Config::new();
    cfg.wasm_simd(true);
    let engine = Engine::new(&cfg).unwrap();
    let module = ReferenceModule::new(&engine, &bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[]).unwrap();
    assert_eq!(
        instance
            .get_typed_func::<(), i32>(&mut store, "l32")
            .unwrap()
            .call(&mut store, ())
            .unwrap(),
        expected32
    );
    assert_eq!(
        instance
            .get_typed_func::<(), i32>(&mut store, "z32")
            .unwrap()
            .call(&mut store, ())
            .unwrap(),
        0
    );
    assert_eq!(
        instance
            .get_typed_func::<(), i64>(&mut store, "l64")
            .unwrap()
            .call(&mut store, ())
            .unwrap(),
        expected64
    );
    assert_eq!(
        instance
            .get_typed_func::<(), i64>(&mut store, "z64")
            .unwrap()
            .call(&mut store, ())
            .unwrap(),
        0
    );
}
