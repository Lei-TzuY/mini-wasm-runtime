use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"(module
  (func (export "low_s") (result i32)
    v128.const i16x8 -32768 32767 -1 1 2 3 -2 -3
    i32x4.extend_low_i16x8_s
    i32x4.extract_lane 0)
  (func (export "high_s") (result i32)
    v128.const i16x8 -32768 32767 -1 1 2 3 -2 -3
    i32x4.extend_high_i16x8_s
    i32x4.extract_lane 2)
  (func (export "low_u") (result i32)
    v128.const i16x8 -32768 32767 -1 1 2 3 -2 -3
    i32x4.extend_low_i16x8_u
    i32x4.extract_lane 2)
  (func (export "high_u") (result i32)
    v128.const i16x8 -32768 32767 -1 1 2 3 -2 -3
    i32x4.extend_high_i16x8_u
    i32x4.extract_lane 3))"#;

const EXPORTS: [&str; 4] = ["low_s", "high_s", "low_u", "high_u"];

fn mini(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini parses i32x4 widening fixture");
    let mut instance = MiniInstance::new(module).expect("mini instantiates i32x4 widening fixture");
    EXPORTS
        .into_iter()
        .map(|name| {
            match instance
                .invoke_export_values(name, &[])
                .expect("mini executes i32x4 widening")
                .as_slice()
            {
                [Value::I32(value)] => *value,
                other => panic!("unexpected mini widening result for {name}: {other:?}"),
            }
        })
        .collect()
}

fn reference(bytes: &[u8]) -> Vec<i32> {
    let mut config = Config::new();
    config.wasm_simd(true);
    let engine = Engine::new(&config).expect("SIMD Wasmtime engine");
    let module =
        ReferenceModule::new(&engine, bytes).expect("Wasmtime compiles i32x4 widening fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime instantiates i32x4 widening fixture");
    EXPORTS
        .into_iter()
        .map(|name| {
            instance
                .get_typed_func::<(), i32>(&mut store, name)
                .expect("typed i32x4 widening export")
                .call(&mut store, ())
                .expect("Wasmtime executes i32x4 widening")
        })
        .collect()
}

#[test]
fn i32x4_widening_matches_wasmtime() {
    let bytes = wat::parse_str(FIXTURE).expect("i32x4 widening WAT parses");
    let expected = vec![-32_768, -2, 65_535, 65_533];
    assert_eq!(mini(&bytes), expected);
    assert_eq!(reference(&bytes), expected);
}
