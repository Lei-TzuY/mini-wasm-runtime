use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "add_sat_s_hi") (result i32)
    v128.const i16x8 32767 0 0 0 0 0 0 0
    v128.const i16x8 1 0 0 0 0 0 0 0
    i16x8.add_sat_s
    i16x8.extract_lane_s 0)
  (func (export "add_sat_s_lo") (result i32)
    v128.const i16x8 -32768 0 0 0 0 0 0 0
    v128.const i16x8 -1 0 0 0 0 0 0 0
    i16x8.add_sat_s
    i16x8.extract_lane_s 0)
  (func (export "add_sat_u_hi") (result i32)
    v128.const i16x8 -1 0 0 0 0 0 0 0
    v128.const i16x8 1 0 0 0 0 0 0 0
    i16x8.add_sat_u
    i16x8.extract_lane_u 0)
  (func (export "sub_sat_s_lo") (result i32)
    v128.const i16x8 -32768 0 0 0 0 0 0 0
    v128.const i16x8 1 0 0 0 0 0 0 0
    i16x8.sub_sat_s
    i16x8.extract_lane_s 0)
  (func (export "sub_sat_s_hi") (result i32)
    v128.const i16x8 32767 0 0 0 0 0 0 0
    v128.const i16x8 -1 0 0 0 0 0 0 0
    i16x8.sub_sat_s
    i16x8.extract_lane_s 0)
  (func (export "sub_sat_u_lo") (result i32)
    v128.const i16x8 0 0 0 0 0 0 0 0
    v128.const i16x8 1 0 0 0 0 0 0 0
    i16x8.sub_sat_u
    i16x8.extract_lane_u 0)
  (func (export "lane_clamped") (result i32)
    v128.const i16x8 100 100 100 100 32767 100 100 100
    v128.const i16x8 10 10 10 10 10 10 10 10
    i16x8.add_sat_s
    i16x8.extract_lane_s 4)
  (func (export "lane_untouched") (result i32)
    v128.const i16x8 100 100 100 100 32767 100 100 100
    v128.const i16x8 10 10 10 10 10 10 10 10
    i16x8.add_sat_s
    i16x8.extract_lane_u 0))
"#;

const EXPORTS: [&str; 8] = [
    "add_sat_s_hi",
    "add_sat_s_lo",
    "add_sat_u_hi",
    "sub_sat_s_lo",
    "sub_sat_s_hi",
    "sub_sat_u_lo",
    "lane_clamped",
    "lane_untouched",
];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini runtime must parse i16x8 saturation fixture");
    let mut instance =
        MiniInstance::new(module).expect("mini runtime must instantiate i16x8 saturation fixture");

    EXPORTS
        .into_iter()
        .map(|export| {
            match instance
                .invoke_export_values(export, &[])
                .expect("mini i16x8 saturation execution must succeed")
                .as_slice()
            {
                [Value::I32(value)] => *value,
                other => panic!("unexpected mini i16x8 saturation result for {export}: {other:?}"),
            }
        })
        .collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i32> {
    let mut config = Config::new();
    config.wasm_simd(true);
    let engine = Engine::new(&config).expect("SIMD-enabled Wasmtime engine must initialize");
    let module = ReferenceModule::new(&engine, bytes)
        .expect("Wasmtime must compile i16x8 saturation fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate i16x8 saturation fixture");

    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i32>(&mut store, export)
                .expect("i16x8 saturation export must be [] -> [i32]")
                .call(&mut store, ())
                .expect("Wasmtime i16x8 saturation execution must succeed")
        })
        .collect()
}

#[test]
fn i16x8_saturating_arithmetic_matches_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("i16x8 saturation WAT fixture must parse");
    let expected = vec![32_767, -32_768, 65_535, -32_768, 32_767, 0, 32_767, 110];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);

    assert_eq!(mini, expected, "mini i16x8 saturation trace drifted");
    assert_eq!(
        reference, expected,
        "Wasmtime i16x8 saturation trace drifted"
    );
    assert_eq!(mini, reference, "i16x8 saturation traces diverged");
}
