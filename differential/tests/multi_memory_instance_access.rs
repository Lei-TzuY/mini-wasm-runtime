use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"(module
  (memory (export "m0") 1 1)
  (memory (export "m1") i64 1 1)
  (data (memory 0) (i32.const 8) "A")
  (data (memory 1) (i64.const 8) "B")
  (func (export "load0") (result i32)
    i32.const 8
    i32.load8_u 0)
  (func (export "load1") (result i32)
    i64.const 8
    i32.load8_u 1))"#;

fn mini_i32(instance: &mut MiniInstance, export: &str) -> i32 {
    match instance
        .invoke_export_values(export, &[])
        .unwrap_or_else(|error| panic!("mini {export} trapped: {error:?}"))
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected mini {export} result: {other:?}"),
    }
}

#[test]
fn indexed_instance_access_matches_wasmtime_for_mixed_memory32_memory64() {
    let bytes = wat::parse_str(FIXTURE).expect("compile mixed multi-memory WAT");

    let parsed = parse_module(&bytes).expect("mini parses mixed multi-memory fixture");
    let mut mini = MiniInstance::new(parsed).expect("mini instantiates mixed multi-memory fixture");
    assert_eq!(mini.memory_count(), 2);
    assert_eq!(mini_i32(&mut mini, "load0"), i32::from(b'A'));
    assert_eq!(mini_i32(&mut mini, "load1"), i32::from(b'B'));
    mini.write_memory_at(1, 8, b"Z")
        .expect("write mini memory64 at index 1");
    assert_eq!(mini.read_memory_at(0, 8, 1).unwrap(), b"A");
    assert_eq!(mini.read_memory_at(1, 8, 1).unwrap(), b"Z");
    assert_eq!(mini_i32(&mut mini, "load0"), i32::from(b'A'));
    assert_eq!(mini_i32(&mut mini, "load1"), i32::from(b'Z'));

    let mut config = Config::new();
    config.wasm_multi_memory(true).wasm_memory64(true);
    let engine = Engine::new(&config).expect("mixed-memory Wasmtime engine");
    let module =
        ReferenceModule::new(&engine, &bytes).expect("Wasmtime compiles mixed-memory fixture");
    let mut store = Store::new(&engine, ());
    let reference =
        ReferenceInstance::new(&mut store, &module, &[]).expect("Wasmtime instantiates fixture");

    let load0 = reference
        .get_typed_func::<(), i32>(&mut store, "load0")
        .expect("Wasmtime load0 export");
    let load1 = reference
        .get_typed_func::<(), i32>(&mut store, "load1")
        .expect("Wasmtime load1 export");
    assert_eq!(load0.call(&mut store, ()).unwrap(), i32::from(b'A'));
    assert_eq!(load1.call(&mut store, ()).unwrap(), i32::from(b'B'));

    let m0 = reference
        .get_memory(&mut store, "m0")
        .expect("Wasmtime memory32 export");
    let m1 = reference
        .get_memory(&mut store, "m1")
        .expect("Wasmtime memory64 export");
    m1.write(&mut store, 8, b"Z")
        .expect("write Wasmtime memory64 at index 1");

    let mut first = [0_u8; 1];
    let mut second = [0_u8; 1];
    m0.read(&store, 8, &mut first).expect("read Wasmtime m0");
    m1.read(&store, 8, &mut second).expect("read Wasmtime m1");
    assert_eq!(first, [b'A']);
    assert_eq!(second, [b'Z']);
    assert_eq!(load0.call(&mut store, ()).unwrap(), i32::from(b'A'));
    assert_eq!(load1.call(&mut store, ()).unwrap(), i32::from(b'Z'));
}
