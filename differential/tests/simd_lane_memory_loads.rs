use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"(module
  (memory 1) (data (i32.const 0) "\01\02\03\04\05\06\07\08")
  (func (export "l8") (result i32) i32.const 0 v128.const i32x4 0 0 0 0 v128.load8_lane 7 i8x16.extract_lane_u 7)
  (func (export "l16") (result i32) i32.const 0 v128.const i32x4 0 0 0 0 v128.load16_lane 3 i16x8.extract_lane_u 3)
  (func (export "l32") (result i32) i32.const 0 v128.const i32x4 0 0 0 0 v128.load32_lane 2 i32x4.extract_lane 2)
  (func (export "l64") (result i64) i32.const 0 v128.const i32x4 0 0 0 0 v128.load64_lane 1 i64x2.extract_lane 1))"#;

#[test]
fn simd_lane_loads_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("WAT parses");
    let module = parse_module(&bytes).expect("mini parses");
    let mut mini = MiniInstance::new(module).expect("mini instantiates");
    let mini_i32 = |instance: &mut MiniInstance, name: &str| match instance
        .invoke_export_values(name, &[])
        .expect("mini executes")
        .as_slice()
    {
        [Value::I32(v)] => *v,
        other => panic!("unexpected {other:?}"),
    };
    assert_eq!(mini_i32(&mut mini, "l8"), 1);
    assert_eq!(mini_i32(&mut mini, "l16"), 0x0201);
    assert_eq!(mini_i32(&mut mini, "l32"), 0x04030201);
    let mini64 = match mini
        .invoke_export_values("l64", &[])
        .expect("mini l64")
        .as_slice()
    {
        [Value::I64(v)] => *v,
        other => panic!("unexpected {other:?}"),
    };
    assert_eq!(mini64, 0x0807060504030201);

    let mut cfg = Config::new();
    cfg.wasm_simd(true);
    let engine = Engine::new(&cfg).unwrap();
    let module = ReferenceModule::new(&engine, &bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[]).unwrap();
    assert_eq!(
        instance
            .get_typed_func::<(), i32>(&mut store, "l8")
            .unwrap()
            .call(&mut store, ())
            .unwrap(),
        1
    );
    assert_eq!(
        instance
            .get_typed_func::<(), i32>(&mut store, "l16")
            .unwrap()
            .call(&mut store, ())
            .unwrap(),
        0x0201
    );
    assert_eq!(
        instance
            .get_typed_func::<(), i32>(&mut store, "l32")
            .unwrap()
            .call(&mut store, ())
            .unwrap(),
        0x04030201
    );
    assert_eq!(
        instance
            .get_typed_func::<(), i64>(&mut store, "l64")
            .unwrap()
            .call(&mut store, ())
            .unwrap(),
        mini64
    );
}
