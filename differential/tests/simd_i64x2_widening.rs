use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"(module
  (func (export "low_s0") (result i64)
    v128.const i32x4 -2147483648 -1 2 2147483647
    i64x2.extend_low_i32x4_s
    i64x2.extract_lane 0)
  (func (export "high_s1") (result i64)
    v128.const i32x4 -2147483648 -1 2 2147483647
    i64x2.extend_high_i32x4_s
    i64x2.extract_lane 1)
  (func (export "low_u1") (result i64)
    v128.const i32x4 -2147483648 -1 2 2147483647
    i64x2.extend_low_i32x4_u
    i64x2.extract_lane 1)
  (func (export "high_u0") (result i64)
    v128.const i32x4 -2147483648 -1 2 2147483647
    i64x2.extend_high_i32x4_u
    i64x2.extract_lane 0))"#;

const EXPORTS: [(&str, i64); 4] = [
    ("low_s0", -2_147_483_648),
    ("high_s1", 2_147_483_647),
    ("low_u1", 4_294_967_295),
    ("high_u0", 2),
];

#[test]
fn i64x2_widening_matches_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("i64x2 widening WAT parses");
    let module = parse_module(&bytes).expect("mini parses i64x2 widening fixture");
    let mut mini = MiniInstance::new(module).expect("mini instantiates i64x2 widening fixture");

    let mut config = Config::new();
    config.wasm_simd(true);
    let engine = Engine::new(&config).expect("SIMD engine");
    let module =
        ReferenceModule::new(&engine, &bytes).expect("Wasmtime compiles i64x2 widening fixture");
    let mut store = Store::new(&engine, ());
    let reference =
        ReferenceInstance::new(&mut store, &module, &[]).expect("Wasmtime instantiates");

    for (name, expected) in EXPORTS {
        let mini_value = match mini
            .invoke_export_values(name, &[])
            .expect("mini i64x2 widening execution")
            .as_slice()
        {
            [Value::I64(value)] => *value,
            other => panic!("unexpected mini widening result for {name}: {other:?}"),
        };
        let reference_value = reference
            .get_typed_func::<(), i64>(&mut store, name)
            .expect("i64x2 widening signature")
            .call(&mut store, ())
            .expect("Wasmtime i64x2 widening execution");
        assert_eq!(mini_value, expected, "mini {name} drifted");
        assert_eq!(reference_value, expected, "Wasmtime {name} drifted");
        assert_eq!(mini_value, reference_value, "{name} differential mismatch");
    }
}
