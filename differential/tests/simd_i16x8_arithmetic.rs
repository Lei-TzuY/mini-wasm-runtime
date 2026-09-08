use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "add_wrap") (result i32)
    v128.const i16x8 32767 -32768 100 -100 1 -1 1234 -1234
    v128.const i16x8 1 -1 23 -23 2 -2 4321 -4321
    i16x8.add
    i16x8.extract_lane_s 0)
  (func (export "sub_wrap") (result i32)
    v128.const i16x8 -32768 32767 100 -100 1 -1 1234 -1234
    v128.const i16x8 1 -1 23 -23 2 -2 4321 -4321
    i16x8.sub
    i16x8.extract_lane_s 0)
  (func (export "mul_wrap") (result i32)
    v128.const i16x8 300 -300 256 -256 17 -17 123 -123
    v128.const i16x8 300 300 257 257 19 19 321 321
    i16x8.mul
    i16x8.extract_lane_u 0)
  (func (export "lane_independent") (result i32)
    v128.const i16x8 10 100 10 10 10 10 10 10
    v128.const i16x8 1 2 1 1 1 1 1 1
    i16x8.add
    i16x8.extract_lane_u 1))
"#;

const EXPORTS: [&str; 4] = ["add_wrap", "sub_wrap", "mul_wrap", "lane_independent"];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini runtime must parse i16x8 arithmetic fixture");
    let mut instance =
        MiniInstance::new(module).expect("mini runtime must instantiate i16x8 arithmetic fixture");

    EXPORTS
        .into_iter()
        .map(|export| {
            match instance
                .invoke_export_values(export, &[])
                .expect("mini i16x8 arithmetic execution must succeed")
                .as_slice()
            {
                [Value::I32(value)] => *value,
                other => panic!("unexpected mini i16x8 arithmetic result for {export}: {other:?}"),
            }
        })
        .collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i32> {
    let mut config = Config::new();
    config.wasm_simd(true);
    let engine = Engine::new(&config).expect("SIMD-enabled Wasmtime engine must initialize");
    let module = ReferenceModule::new(&engine, bytes)
        .expect("Wasmtime must compile i16x8 arithmetic fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate i16x8 arithmetic fixture");

    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i32>(&mut store, export)
                .expect("i16x8 arithmetic export must be [] -> [i32]")
                .call(&mut store, ())
                .expect("Wasmtime i16x8 arithmetic execution must succeed")
        })
        .collect()
}

#[test]
fn i16x8_wrapping_arithmetic_matches_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("i16x8 arithmetic WAT fixture must parse");
    let expected = vec![-32_768, 32_767, 24_464, 102];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);

    assert_eq!(mini, expected, "mini i16x8 arithmetic trace drifted");
    assert_eq!(
        reference, expected,
        "Wasmtime i16x8 arithmetic trace drifted"
    );
    assert_eq!(mini, reference, "i16x8 arithmetic traces diverged");
}
