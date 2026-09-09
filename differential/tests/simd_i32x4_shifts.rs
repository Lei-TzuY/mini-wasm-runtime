use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "shl") (result i32)
    i32.const 1073741824 i32x4.splat i32.const 33 i32x4.shl i32x4.extract_lane 0)
  (func (export "shr_s") (result i32)
    i32.const -2 i32x4.splat i32.const 1 i32x4.shr_s i32x4.extract_lane 0)
  (func (export "shr_u") (result i32)
    i32.const -2147483648 i32x4.splat i32.const 1 i32x4.shr_u i32x4.extract_lane 0))
"#;
const EXPORTS: [&str; 3] = ["shl", "shr_s", "shr_u"];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini must parse i32x4 shift fixture");
    let mut instance =
        MiniInstance::new(module).expect("mini must instantiate i32x4 shift fixture");
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
    let instance = ReferenceInstance::new(&mut store, &module, &[]).expect("Wasmtime instantiate");
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
fn i32x4_shifts_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("fixture parse");
    let expected = vec![i32::MIN, -1, 0x4000_0000];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, expected);
    assert_eq!(reference, expected);
    assert_eq!(mini, reference);
}
