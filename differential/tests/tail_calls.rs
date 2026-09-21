use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"(module
  (type $unary (func (param i32) (result i32)))
  (type $multi (func (param i32) (result i32 i64)))

  (func $recurse (type $unary) (param $n i32) (result i32)
    local.get $n
    i32.eqz
    if (result i32)
      i32.const 17
    else
      local.get $n
      i32.const 1
      i32.sub
      return_call $recurse
    end)

  (func $plus5 (type $unary) (param $x i32) (result i32)
    local.get $x
    i32.const 5
    i32.add)

  (table 1 funcref)
  (elem (i32.const 0) $plus5)

  (func (export "indirect") (type $unary) (param $x i32) (result i32)
    local.get $x
    i32.const 0
    return_call_indirect (type $unary))

  (func $multi_target (type $multi) (param $x i32) (result i32 i64)
    local.get $x
    i64.const 9)

  (func (export "multi") (type $multi) (param $x i32) (result i32 i64)
    local.get $x
    return_call $multi_target)

  (export "direct" (func $recurse)))"#;

fn engine() -> Engine {
    let mut config = Config::new();
    config.wasm_tail_call(true);
    Engine::new(&config).expect("tail-call reference engine")
}

#[test]
fn tail_call_results_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("tail-call WAT parses");

    let parsed = parse_module(&bytes).expect("mini parses tail-call fixture");
    let mut mini = MiniInstance::new(parsed).expect("mini instantiates tail-call fixture");

    let engine = engine();
    let module =
        ReferenceModule::new(&engine, &bytes).expect("Wasmtime compiles tail-call fixture");
    let mut store = Store::new(&engine, ());
    let reference =
        ReferenceInstance::new(&mut store, &module, &[]).expect("Wasmtime instantiates");

    let mini_direct = mini
        .invoke_export_values("direct", &[Value::I32(1_000)])
        .expect("mini direct tail recursion");
    let reference_direct = reference
        .get_typed_func::<i32, i32>(&mut store, "direct")
        .expect("reference direct signature")
        .call(&mut store, 1_000)
        .expect("reference direct tail recursion");
    assert_eq!(mini_direct, vec![Value::I32(reference_direct)]);

    let mini_indirect = mini
        .invoke_export_values("indirect", &[Value::I32(37)])
        .expect("mini indirect tail call");
    let reference_indirect = reference
        .get_typed_func::<i32, i32>(&mut store, "indirect")
        .expect("reference indirect signature")
        .call(&mut store, 37)
        .expect("reference indirect tail call");
    assert_eq!(mini_indirect, vec![Value::I32(reference_indirect)]);
    assert_eq!(mini_indirect, vec![Value::I32(42)]);

    let mini_multi = mini
        .invoke_export_values("multi", &[Value::I32(7)])
        .expect("mini multi-value tail call");
    let reference_multi = reference
        .get_typed_func::<i32, (i32, i64)>(&mut store, "multi")
        .expect("reference multi-value signature")
        .call(&mut store, 7)
        .expect("reference multi-value tail call");
    assert_eq!(
        mini_multi,
        vec![Value::I32(reference_multi.0), Value::I64(reference_multi.1)]
    );
    assert_eq!(mini_multi, vec![Value::I32(7), Value::I64(9)]);
}
