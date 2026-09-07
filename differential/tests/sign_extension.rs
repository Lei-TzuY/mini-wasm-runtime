use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

fn run_i32(wat_source: &str) -> (i32, i32) {
    let bytes = wat::parse_str(wat_source).expect("fixture must parse as WAT");

    let module = parse_module(&bytes).expect("mini runtime must parse fixture");
    let mut mini = MiniInstance::new(module).expect("mini runtime must instantiate fixture");
    let mini_value = match mini
        .invoke_export_values("run", &[])
        .expect("mini runtime execution must succeed")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        values => panic!("unexpected mini-runtime result shape: {values:?}"),
    };

    let engine = Engine::default();
    let module = ReferenceModule::new(&engine, &bytes).expect("Wasmtime must compile fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate fixture");
    let run = instance
        .get_typed_func::<(), i32>(&mut store, "run")
        .expect("run export must have [] -> [i32] signature");
    let reference_value = run
        .call(&mut store, ())
        .expect("Wasmtime execution must succeed");

    (mini_value, reference_value)
}

fn run_i64(wat_source: &str) -> (i64, i64) {
    let bytes = wat::parse_str(wat_source).expect("fixture must parse as WAT");

    let module = parse_module(&bytes).expect("mini runtime must parse fixture");
    let mut mini = MiniInstance::new(module).expect("mini runtime must instantiate fixture");
    let mini_value = match mini
        .invoke_export_values("run", &[])
        .expect("mini runtime execution must succeed")
        .as_slice()
    {
        [Value::I64(value)] => *value,
        values => panic!("unexpected mini-runtime result shape: {values:?}"),
    };

    let engine = Engine::default();
    let module = ReferenceModule::new(&engine, &bytes).expect("Wasmtime must compile fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate fixture");
    let run = instance
        .get_typed_func::<(), i64>(&mut store, "run")
        .expect("run export must have [] -> [i64] signature");
    let reference_value = run
        .call(&mut store, ())
        .expect("Wasmtime execution must succeed");

    (mini_value, reference_value)
}

#[test]
fn sign_extension_matches_wasmtime_reference() {
    let i32_cases = [
        r#"(module (func (export "run") (result i32) i32.const 128 i32.extend8_s))"#,
        r#"(module (func (export "run") (result i32) i32.const 32768 i32.extend16_s))"#,
    ];
    for wat_source in i32_cases {
        let (mini, reference) = run_i32(wat_source);
        assert_eq!(mini, reference);
    }

    let i64_cases = [
        r#"(module (func (export "run") (result i64) i64.const 255 i64.extend8_s))"#,
        r#"(module (func (export "run") (result i64) i64.const 32768 i64.extend16_s))"#,
        r#"(module (func (export "run") (result i64) i64.const 4294967295 i64.extend32_s))"#,
    ];
    for wat_source in i64_cases {
        let (mini, reference) = run_i64(wat_source);
        assert_eq!(mini, reference);
    }
}
