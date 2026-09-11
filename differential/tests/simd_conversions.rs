use wasm_runtime::{Instance, Value};
use wasmtime::{Engine, Instance as WasmtimeInstance, Module as WasmtimeModule, Store};

const FIXTURE: &str = r#"(module
  (memory 1)
  (func (export "trunc_s") (result i32)
    i32.const 0 v128.const f32x4 42.9 nan inf -inf i32x4.trunc_sat_f32x4_s v128.store
    i32.const 0 i32.load)
  (func (export "trunc_u") (result i32)
    i32.const 0 v128.const f32x4 -1 42.9 nan inf i32x4.trunc_sat_f32x4_u v128.store
    i32.const 4 i32.load)
  (func (export "convert_s_bits") (result i32)
    i32.const 0 v128.const i32x4 -7 9 0 1 f32x4.convert_i32x4_s v128.store
    i32.const 0 i32.load)
  (func (export "trunc_f64_zero") (result i32)
    i32.const 0 v128.const f64x2 -19.75 33.5 i32x4.trunc_sat_f64x2_s_zero v128.store
    i32.const 8 i32.load)
  (func (export "convert_low_u_bits") (result i64)
    i32.const 0 v128.const i32x4 4294967295 17 9 11 f64x2.convert_low_i32x4_u v128.store
    i32.const 0 i64.load))"#;

#[test]
fn terminal_simd_conversions_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("wat");
    let parsed = wasm_parser::parse_module(&bytes).expect("parse");
    let mut mini = Instance::new(parsed).expect("mini");

    let mini_i32 =
        ["trunc_s", "trunc_u", "convert_s_bits", "trunc_f64_zero"].map(|name| {
            match mini.invoke_export_values(name, &[]).unwrap().as_slice() {
                [Value::I32(value)] => *value,
                other => panic!("unexpected mini i32 result for {name}: {other:?}"),
            }
        });
    let mini_i64 = match mini
        .invoke_export_values("convert_low_u_bits", &[])
        .unwrap()
        .as_slice()
    {
        [Value::I64(value)] => *value,
        other => panic!("unexpected mini i64 result: {other:?}"),
    };

    let engine = Engine::default();
    let module = WasmtimeModule::new(&engine, &bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = WasmtimeInstance::new(&mut store, &module, &[]).unwrap();
    let reference_i32 = ["trunc_s", "trunc_u", "convert_s_bits", "trunc_f64_zero"].map(|name| {
        instance
            .get_typed_func::<(), i32>(&mut store, name)
            .unwrap()
            .call(&mut store, ())
            .unwrap()
    });
    let reference_i64 = instance
        .get_typed_func::<(), i64>(&mut store, "convert_low_u_bits")
        .unwrap()
        .call(&mut store, ())
        .unwrap();

    assert_eq!(mini_i32, reference_i32);
    assert_eq!(mini_i64, reference_i64);
}
