use wasm_parser::{parse_module, ValueType};
use wasm_runtime::{
    HostCapabilities, HostRegistry, HostRegistryError, Instance, RuntimeError, Value,
};
use wasmtime::{Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

fn execute_mini(wat_source: &str, export: &str) -> Result<Vec<Value>, String> {
    let wasm = wat::parse_str(wat_source).map_err(|error| error.to_string())?;
    let module = parse_module(&wasm).map_err(|error| error.to_string())?;
    let mut instance = Instance::new(module).map_err(|error| error.to_string())?;
    instance
        .invoke_export_values(export, &[])
        .map_err(|error| error.to_string())
}

fn execute_wasmtime_i32(wat_source: &str, export: &str) -> i32 {
    let engine = Engine::default();
    let module = ReferenceModule::new(&engine, wat_source).expect("reference module compiles");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[]).expect("reference instance");
    instance
        .get_typed_func::<(), i32>(&mut store, export)
        .expect("typed reference export")
        .call(&mut store, ())
        .expect("reference invocation")
}

#[test]
fn defined_funcref_params_results_and_locals_match_wasmtime() {
    let module = r#"
        (module
          (type $ref_id (func (param funcref) (result funcref)))
          (func $target)
          (elem declare func $target)
          (func $id (type $ref_id) (param funcref) (result funcref)
            local.get 0)
          (func (export "roundtrip_is_null") (result i32)
            ref.null func
            call $id
            ref.is_null)
          (func (export "default_local_is_null") (result i32)
            (local funcref)
            local.get 0
            ref.is_null)
          (func (export "non_null_survives_local") (result i32)
            (local funcref)
            ref.func $target
            local.set 0
            local.get 0
            ref.is_null))
    "#;

    for (export, expected) in [
        ("roundtrip_is_null", 1),
        ("default_local_is_null", 1),
        ("non_null_survives_local", 0),
    ] {
        let mini = execute_mini(module, export)
            .expect("mini runtime executes reference-typed defined function");
        assert_eq!(mini, vec![Value::I32(expected)]);
        assert_eq!(execute_wasmtime_i32(module, export), expected);
    }
}

#[test]
fn imported_reference_typed_function_remains_fail_closed() {
    let module = r#"
        (module
          (type $host_ref (func (param funcref) (result funcref)))
          (import "host" "ref_id" (func (type $host_ref))))
    "#;
    let wasm = wat::parse_str(module).expect("reference-typed import encodes");
    let parsed = parse_module(&wasm)
        .expect("type syntax parses once defined funcref signatures are supported");

    let mut hosts = HostRegistry::new();
    let registration = hosts.register_values(
        "host",
        "ref_id",
        vec![ValueType::FuncRef],
        vec![ValueType::FuncRef],
        HostCapabilities::NONE,
        |_context, args| Ok(args.to_vec()),
    );
    assert_eq!(registration, Err(HostRegistryError::UnsupportedSignature));

    assert!(matches!(
        Instance::with_hosts(parsed, hosts),
        Err(RuntimeError::UnresolvedImport { module, name })
            if module == "host" && name == "ref_id"
    ));
}
