use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"(module
  (memory 1)
  (func (export "s8") (result i32) i32.const 0 v128.const i8x16 17 34 51 68 85 102 119 -120 -103 -86 -69 -52 -35 -18 -1 16 v128.store8_lane 7 i32.const 0 i32.load8_u)
  (func (export "s16") (result i32) i32.const 0 v128.const i16x8 8721 17459 26197 -30601 -21829 -13091 -4353 4351 v128.store16_lane 3 i32.const 0 i32.load16_u)
  (func (export "s32") (result i32) i32.const 0 v128.const i32x4 1144201745 -2005440939 -860116327 285208285 v128.store32_lane 2 i32.const 0 i32.load)
  (func (export "s64") (result i64) i32.const 0 v128.const i64x2 -8613303245920329199 1224958050675874979 v128.store64_lane 1 i32.const 0 i64.load))"#;

#[test]
fn simd_lane_stores_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("WAT parses");
    let module = parse_module(&bytes).expect("mini parses");
    let mut mini = MiniInstance::new(module).expect("mini instantiates");
    let names = ["s8", "s16", "s32"];
    let mut mini_i32 = Vec::new();
    for name in names {
        let value = match mini
            .invoke_export_values(name, &[])
            .expect("mini executes")
            .as_slice()
        {
            [Value::I32(v)] => *v,
            other => panic!("unexpected {other:?}"),
        };
        mini_i32.push(value);
    }
    let mini_i64 = match mini
        .invoke_export_values("s64", &[])
        .expect("mini executes")
        .as_slice()
    {
        [Value::I64(v)] => *v,
        other => panic!("unexpected {other:?}"),
    };

    let mut cfg = Config::new();
    cfg.wasm_simd(true);
    let engine = Engine::new(&cfg).unwrap();
    let module = ReferenceModule::new(&engine, &bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[]).unwrap();
    for (idx, name) in names.into_iter().enumerate() {
        let got = instance
            .get_typed_func::<(), i32>(&mut store, name)
            .unwrap()
            .call(&mut store, ())
            .unwrap();
        assert_eq!(got, mini_i32[idx]);
    }
    let got = instance
        .get_typed_func::<(), i64>(&mut store, "s64")
        .unwrap()
        .call(&mut store, ())
        .unwrap();
    assert_eq!(got, mini_i64);
}
