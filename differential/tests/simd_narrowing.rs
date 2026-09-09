use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"(module
  (func (export "i8s") (result i32)
    v128.const i16x8 -200 -129 -128 0 127 128 200 32767
    v128.const i16x8 -32768 -1 1 2 3 4 5 6
    i8x16.narrow_i16x8_s
    i8x16.extract_lane_s 8)
  (func (export "i8u") (result i32)
    v128.const i16x8 -1 0 1 255 256 300 32767 42
    v128.const i16x8 700 2 3 4 5 6 7 8
    i8x16.narrow_i16x8_u
    i8x16.extract_lane_u 0)
  (func (export "i16s") (result i32)
    v128.const i32x4 -40000 -32768 32767 40000
    v128.const i32x4 -2147483648 -1 1 2147483647
    i16x8.narrow_i32x4_s
    i16x8.extract_lane_s 7)
  (func (export "extend_low_s") (result i32)
    v128.const i8x16 -128 127 -1 1 2 3 4 5 6 7 8 9 -2 -3 -4 -5
    i16x8.extend_low_i8x16_s
    i16x8.extract_lane_s 0)
  (func (export "extend_low_u") (result i32)
    v128.const i8x16 -128 127 -1 1 2 3 4 5 6 7 8 9 -2 -3 -4 -5
    i16x8.extend_low_i8x16_u
    i16x8.extract_lane_u 2)
  (func (export "extend_high_s") (result i32)
    v128.const i8x16 -128 127 -1 1 2 3 4 5 6 7 8 9 -2 -3 -4 -5
    i16x8.extend_high_i8x16_s
    i16x8.extract_lane_s 4)
  (func (export "extend_high_u") (result i32)
    v128.const i8x16 -128 127 -1 1 2 3 4 5 6 7 8 9 -2 -3 -4 -5
    i16x8.extend_high_i8x16_u
    i16x8.extract_lane_u 7)
  (func (export "i16u") (result i32)
    v128.const i32x4 -1 0 65535 70000
    v128.const i32x4 1 2 3 4
    i16x8.narrow_i32x4_u
    i16x8.extract_lane_u 3))"#;
const EXPORTS: [&str; 8] = [
    "i8s",
    "i8u",
    "i16s",
    "extend_low_s",
    "extend_low_u",
    "extend_high_s",
    "extend_high_u",
    "i16u",
];
fn mini(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini parses narrowing fixture");
    let mut instance = MiniInstance::new(module).expect("mini instantiates narrowing fixture");
    EXPORTS
        .into_iter()
        .map(|name| {
            match instance
                .invoke_export_values(name, &[])
                .expect("mini executes narrowing")
                .as_slice()
            {
                [Value::I32(v)] => *v,
                other => panic!("unexpected mini narrowing result for {name}: {other:?}"),
            }
        })
        .collect()
}
fn reference(bytes: &[u8]) -> Vec<i32> {
    let mut config = Config::new();
    config.wasm_simd(true);
    let engine = Engine::new(&config).expect("SIMD Wasmtime engine");
    let module = ReferenceModule::new(&engine, bytes).expect("Wasmtime compiles narrowing fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime instantiates narrowing fixture");
    EXPORTS
        .into_iter()
        .map(|name| {
            instance
                .get_typed_func::<(), i32>(&mut store, name)
                .expect("typed narrowing export")
                .call(&mut store, ())
                .expect("Wasmtime executes narrowing")
        })
        .collect()
}
#[test]
fn narrowing_matches_wasmtime() {
    let bytes = wat::parse_str(FIXTURE).expect("narrowing WAT parses");
    let expected = vec![-128, 0, 32767, -128, 255, -2, 251, 65535];
    assert_eq!(mini(&bytes), expected);
    assert_eq!(reference(&bytes), expected);
}
