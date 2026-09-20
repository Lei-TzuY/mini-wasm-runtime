use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{
    Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Ref, Store, Val,
};

const FIXTURE: &str = r#"(module
  (type $target (func (result i32)))
  (memory (export "mem") 1 1)
  (global (export "g") (mut i32) (i32.const 5))
  (table (export "tab") 2 2 funcref)
  (func $a (type $target) (result i32) i32.const 11)
  (func $b (type $target) (result i32) i32.const 22)
  (elem (i32.const 0) $a $b)
  (data (i32.const 0) "A")
  (func (export "load") (result i32)
    i32.const 0
    i32.load8_u)
  (func (export "read_g") (result i32)
    global.get 0)
  (func (export "call") (param i32) (result i32)
    local.get 0
    call_indirect (type $target)))"#;

fn mini_i32(instance: &mut MiniInstance, export: &str, args: &[Value]) -> i32 {
    match instance
        .invoke_export_values(export, args)
        .unwrap_or_else(|error| panic!("mini {export} trapped: {error:?}"))
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected mini {export} result: {other:?}"),
    }
}

#[test]
fn exported_state_host_mutation_matches_wasmtime() {
    let bytes = wat::parse_str(FIXTURE).expect("compile exported-state WAT");
    let parsed = parse_module(&bytes).expect("mini parses exported-state fixture");
    let mut mini = MiniInstance::new(parsed).expect("mini instantiates exported-state fixture");

    let mini_memory = mini.exported_memory_index("mem").unwrap();
    let mini_global = mini.exported_global("g").unwrap();
    let mini_table = mini.exported_table("tab").unwrap();

    mini.write_memory_at(mini_memory, 0, b"Z").unwrap();
    mini_global.set(Value::I32(9)).unwrap();
    let mini_slot_zero = mini_table
        .get(0)
        .unwrap()
        .expect("mini table slot 0 initialized");
    mini_table.set(1, Some(mini_slot_zero)).unwrap();

    assert_eq!(mini_i32(&mut mini, "load", &[]), i32::from(b'Z'));
    assert_eq!(mini_i32(&mut mini, "read_g", &[]), 9);
    assert_eq!(mini_i32(&mut mini, "call", &[Value::I32(1)]), 11);

    let mut config = Config::new();
    config.wasm_multi_memory(true).wasm_memory64(true);
    let engine = Engine::new(&config).expect("Wasmtime exported-state engine");
    let module = ReferenceModule::new(&engine, &bytes).expect("Wasmtime compiles fixture");
    let mut store = Store::new(&engine, ());
    let reference =
        ReferenceInstance::new(&mut store, &module, &[]).expect("Wasmtime instantiates fixture");

    let memory = reference
        .get_memory(&mut store, "mem")
        .expect("Wasmtime memory export");
    let global = reference
        .get_global(&mut store, "g")
        .expect("Wasmtime global export");
    let table = reference
        .get_table(&mut store, "tab")
        .expect("Wasmtime table export");

    memory.write(&mut store, 0, b"Z").unwrap();
    global.set(&mut store, Val::I32(9)).unwrap();
    let slot_zero = table
        .get(&mut store, 0)
        .expect("Wasmtime table slot 0 initialized");
    assert!(matches!(slot_zero, Ref::Func(Some(_))));
    table.set(&mut store, 1, slot_zero).unwrap();

    let load = reference
        .get_typed_func::<(), i32>(&mut store, "load")
        .expect("Wasmtime load export");
    let read_g = reference
        .get_typed_func::<(), i32>(&mut store, "read_g")
        .expect("Wasmtime global reader export");
    let call = reference
        .get_typed_func::<i32, i32>(&mut store, "call")
        .expect("Wasmtime table call export");

    assert_eq!(load.call(&mut store, ()).unwrap(), i32::from(b'Z'));
    assert_eq!(read_g.call(&mut store, ()).unwrap(), 9);
    assert_eq!(call.call(&mut store, 1).unwrap(), 11);
}
