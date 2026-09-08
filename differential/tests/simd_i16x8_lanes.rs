use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "signed_neg1") (result i32)
    i32.const -1
    i16x8.splat
    i16x8.extract_lane_s 3)
  (func (export "unsigned_neg1") (result i32)
    i32.const -1
    i16x8.splat
    i16x8.extract_lane_u 6)
  (func (export "truncate_splat") (result i32)
    i32.const 74565
    i16x8.splat
    i16x8.extract_lane_u 0)
  (func (export "replace_truncates") (result i32)
    i32.const 7
    i16x8.splat
    i32.const 131071
    i16x8.replace_lane 7
    i16x8.extract_lane_u 7)
  (func (export "replace_preserves_other_lane") (result i32)
    i32.const 7
    i16x8.splat
    i32.const 131071
    i16x8.replace_lane 7
    i16x8.extract_lane_u 0)
  (func (export "structured_signed_min") (result i32)
    block (result i32)
      i32.const -32768
      i16x8.splat
      i16x8.extract_lane_s 5
    end)
)
"#;

const EXPORTS: [&str; 6] = [
    "signed_neg1",
    "unsigned_neg1",
    "truncate_splat",
    "replace_truncates",
    "replace_preserves_other_lane",
    "structured_signed_min",
];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini runtime must parse i16x8 lane fixture");
    let mut instance =
        MiniInstance::new(module).expect("mini runtime must instantiate i16x8 lane fixture");

    EXPORTS
        .into_iter()
        .map(|export| {
            let values = instance
                .invoke_export_values(export, &[])
                .expect("mini i16x8 lane execution must succeed");
            match values.as_slice() {
                [Value::I32(value)] => *value,
                other => panic!("unexpected mini i16x8 lane result for {export}: {other:?}"),
            }
        })
        .collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i32> {
    let mut config = Config::new();
    config.wasm_simd(true);
    let engine = Engine::new(&config).expect("SIMD-enabled Wasmtime engine must initialize");
    let module =
        ReferenceModule::new(&engine, bytes).expect("Wasmtime must compile i16x8 lane fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate i16x8 lane fixture");

    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i32>(&mut store, export)
                .expect("i16x8 lane export must be [] -> [i32]")
                .call(&mut store, ())
                .expect("Wasmtime i16x8 lane execution must succeed")
        })
        .collect()
}

#[test]
fn i16x8_lane_primitives_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("i16x8 lane WAT fixture must parse");
    let expected = vec![-1, 65_535, 0x2345, 65_535, 7, -32_768];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);

    assert_eq!(mini, expected, "mini i16x8 lane trace drifted");
    assert_eq!(reference, expected, "Wasmtime i16x8 lane trace drifted");
    assert_eq!(mini, reference, "i16x8 lane traces diverged");
}
