use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "abs") (result i32)
    v128.const i16x8 -123 0 0 0 0 0 0 0
    i16x8.abs
    i16x8.extract_lane_s 0)
  (func (export "abs_min") (result i32)
    v128.const i16x8 -32768 0 0 0 0 0 0 0
    i16x8.abs
    i16x8.extract_lane_s 0)
  (func (export "neg") (result i32)
    v128.const i16x8 123 0 0 0 0 0 0 0
    i16x8.neg
    i16x8.extract_lane_s 0)
  (func (export "q15_sat") (result i32)
    v128.const i16x8 -32768 0 0 0 0 0 0 0
    v128.const i16x8 -32768 0 0 0 0 0 0 0
    i16x8.q15mulr_sat_s
    i16x8.extract_lane_s 0)
  (func (export "q15_half") (result i32)
    v128.const i16x8 16384 0 0 0 0 0 0 0
    v128.const i16x8 16384 0 0 0 0 0 0 0
    i16x8.q15mulr_sat_s
    i16x8.extract_lane_s 0)
  (func (export "all_true") (result i32)
    v128.const i16x8 1 -1 2 -2 3 -3 4 -4
    i16x8.all_true)
  (func (export "not_all_true") (result i32)
    v128.const i16x8 1 -1 2 0 3 -3 4 -4
    i16x8.all_true)
  (func (export "bitmask") (result i32)
    v128.const i16x8 -1 0 1 -32768 32767 0 1 -2
    i16x8.bitmask)
  (func (export "shl_masked") (result i32)
    v128.const i16x8 16384 0 0 0 0 0 0 0
    i32.const 17
    i16x8.shl
    i16x8.extract_lane_s 0)
  (func (export "shr_s") (result i32)
    v128.const i16x8 -2 0 0 0 0 0 0 0
    i32.const 1
    i16x8.shr_s
    i16x8.extract_lane_s 0)
  (func (export "shr_u") (result i32)
    v128.const i16x8 -32768 0 0 0 0 0 0 0
    i32.const 1
    i16x8.shr_u
    i16x8.extract_lane_u 0)
  (func (export "min_s") (result i32)
    v128.const i16x8 -1 7 300 -400 5 6 7 8
    v128.const i16x8 1 6 -300 -399 10 5 8 7
    i16x8.min_s
    i16x8.extract_lane_s 2)
  (func (export "min_u") (result i32)
    v128.const i16x8 -1 0 0 0 0 0 0 0
    v128.const i16x8 1 0 0 0 0 0 0 0
    i16x8.min_u
    i16x8.extract_lane_u 0)
  (func (export "max_s") (result i32)
    v128.const i16x8 -1 0 0 0 0 0 0 0
    v128.const i16x8 1 0 0 0 0 0 0 0
    i16x8.max_s
    i16x8.extract_lane_s 0)
  (func (export "max_u") (result i32)
    v128.const i16x8 -1 0 0 0 0 0 0 0
    v128.const i16x8 1 0 0 0 0 0 0 0
    i16x8.max_u
    i16x8.extract_lane_u 0)
  (func (export "avgr") (result i32)
    v128.const i16x8 1 0 0 0 0 0 0 0
    v128.const i16x8 2 0 0 0 0 0 0 0
    i16x8.avgr_u
    i16x8.extract_lane_u 0)
  (func (export "avgr_hi") (result i32)
    v128.const i16x8 -1 0 0 0 0 0 0 0
    v128.const i16x8 -2 0 0 0 0 0 0 0
    i16x8.avgr_u
    i16x8.extract_lane_u 0))
"#;

const EXPORTS: [&str; 17] = [
    "abs",
    "abs_min",
    "neg",
    "q15_sat",
    "q15_half",
    "all_true",
    "not_all_true",
    "bitmask",
    "shl_masked",
    "shr_s",
    "shr_u",
    "min_s",
    "min_u",
    "max_s",
    "max_u",
    "avgr",
    "avgr_hi",
];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini runtime must parse i16x8 ALU fixture");
    let mut instance =
        MiniInstance::new(module).expect("mini runtime must instantiate i16x8 ALU fixture");

    EXPORTS
        .into_iter()
        .map(|export| {
            match instance
                .invoke_export_values(export, &[])
                .expect("mini i16x8 ALU execution must succeed")
                .as_slice()
            {
                [Value::I32(value)] => *value,
                other => panic!("unexpected mini i16x8 ALU result for {export}: {other:?}"),
            }
        })
        .collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i32> {
    let mut config = Config::new();
    config.wasm_simd(true);
    let engine = Engine::new(&config).expect("SIMD-enabled Wasmtime engine must initialize");
    let module =
        ReferenceModule::new(&engine, bytes).expect("Wasmtime must compile i16x8 ALU fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate i16x8 ALU fixture");

    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i32>(&mut store, export)
                .expect("i16x8 ALU export must be [] -> [i32]")
                .call(&mut store, ())
                .expect("Wasmtime i16x8 ALU execution must succeed")
        })
        .collect()
}

#[test]
fn i16x8_scalar_alu_closure_matches_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("i16x8 ALU WAT fixture must parse");
    let expected = vec![
        123,
        -32_768,
        -123,
        32_767,
        8_192,
        1,
        0,
        0b1000_1001,
        -32_768,
        -1,
        16_384,
        -300,
        1,
        1,
        65_535,
        2,
        65_535,
    ];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);

    assert_eq!(mini, expected, "mini i16x8 ALU trace drifted");
    assert_eq!(reference, expected, "Wasmtime i16x8 ALU trace drifted");
    assert_eq!(mini, reference, "i16x8 ALU traces diverged");
}
