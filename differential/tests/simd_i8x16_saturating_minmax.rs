use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{
    Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store,
};

const FIXTURE: &str = r#"
(module
  (func (export "add_sat_s") (result i32)
    i32.const 127 i8x16.splat i32.const 1 i8x16.splat
    i8x16.add_sat_s i8x16.extract_lane_s 0)
  (func (export "sub_sat_s") (result i32)
    i32.const -128 i8x16.splat i32.const 1 i8x16.splat
    i8x16.sub_sat_s i8x16.extract_lane_s 0)
  (func (export "add_sat_u") (result i32)
    i32.const 255 i8x16.splat i32.const 1 i8x16.splat
    i8x16.add_sat_u i8x16.extract_lane_u 0)
  (func (export "sub_sat_u") (result i32)
    i32.const 0 i8x16.splat i32.const 1 i8x16.splat
    i8x16.sub_sat_u i8x16.extract_lane_u 0)
  (func (export "min_s") (result i32)
    i32.const -1 i8x16.splat i32.const 1 i8x16.splat
    i8x16.min_s i8x16.extract_lane_s 0)
  (func (export "max_s") (result i32)
    i32.const -1 i8x16.splat i32.const 1 i8x16.splat
    i8x16.max_s i8x16.extract_lane_s 0)
  (func (export "min_u") (result i32)
    i32.const 255 i8x16.splat i32.const 1 i8x16.splat
    i8x16.min_u i8x16.extract_lane_u 0)
  (func (export "max_u") (result i32)
    i32.const 255 i8x16.splat i32.const 1 i8x16.splat
    i8x16.max_u i8x16.extract_lane_u 0)
  (func (export "avgr_u") (result i32)
    i32.const 10 i8x16.splat i32.const 13 i8x16.splat
    i8x16.avgr_u i8x16.extract_lane_u 0))
"#;

const EXPORTS: [&str; 9] = [
    "add_sat_s",
    "sub_sat_s",
    "add_sat_u",
    "sub_sat_u",
    "min_s",
    "max_s",
    "min_u",
    "max_u",
    "avgr_u",
];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini must parse i8x16 ALU fixture");
    let mut instance = MiniInstance::new(module).expect("mini must instantiate i8x16 ALU fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            match instance
                .invoke_export_values(export, &[])
                .expect("mini execution")
                .as_slice()
            {
                [Value::I32(value)] => *value,
                other => panic!("unexpected mini result for {export}: {other:?}"),
            }
        })
        .collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i32> {
    let mut config = Config::new();
    config.wasm_simd(true);
    let engine = Engine::new(&config).expect("SIMD Wasmtime engine");
    let module = ReferenceModule::new(&engine, bytes).expect("Wasmtime compile");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime instantiate");
    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i32>(&mut store, export)
                .expect("signature")
                .call(&mut store, ())
                .expect("reference execution")
        })
        .collect()
}

#[test]
fn i8x16_saturating_minmax_matches_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("fixture parse");
    let expected = vec![127, -128, 255, 0, -1, 1, 1, 255, 12];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, expected);
    assert_eq!(reference, expected);
    assert_eq!(mini, reference);
}
