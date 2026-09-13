use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "all_a") (result i32)
    v128.const i32x4 -1431655766 -1431655766 -1431655766 -1431655766
    v128.const i32x4 1431655765 1431655765 1431655765 1431655765
    v128.const i32x4 -1 -1 -1 -1
    i32x4.relaxed_laneselect
    i32x4.extract_lane 0)
  (func (export "all_b") (result i32)
    v128.const i32x4 -1431655766 -1431655766 -1431655766 -1431655766
    v128.const i32x4 1431655765 1431655765 1431655765 1431655765
    v128.const i32x4 0 0 0 0
    i32x4.relaxed_laneselect
    i32x4.extract_lane 0))
"#;
const EXPORTS: [&str; 2] = ["all_a", "all_b"];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini runtime parses relaxed lane-select fixture");
    let mut instance =
        MiniInstance::new(module).expect("mini runtime instantiates relaxed lane-select fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            match instance
                .invoke_export_values(export, &[])
                .expect("mini lane-select executes")
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
    config.wasm_relaxed_simd(true);
    let engine = Engine::new(&config).expect("Wasmtime engine initializes");
    let module = ReferenceModule::new(&engine, bytes)
        .expect("Wasmtime compiles relaxed lane-select fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime instantiates relaxed lane-select fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i32>(&mut store, export)
                .expect("lane-select export is [] -> [i32]")
                .call(&mut store, ())
                .expect("Wasmtime lane-select executes")
        })
        .collect()
}

#[test]
fn relaxed_i32x4_laneselect_matches_wasmtime_for_deterministic_masks() {
    let bytes = wat::parse_str(FIXTURE).expect("relaxed lane-select WAT parses");
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, vec![-1431655766, 1431655765]);
    assert_eq!(reference, vec![-1431655766, 1431655765]);
    assert_eq!(mini, reference);
}
