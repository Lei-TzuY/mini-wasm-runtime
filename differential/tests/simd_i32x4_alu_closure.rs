use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"(module
  (func (export "abs_min") (result i32)
    v128.const i32x4 -2147483648 -7 0 9
    i32x4.abs
    i32x4.extract_lane 0)
  (func (export "neg_3") (result i32)
    v128.const i32x4 -2147483648 -7 0 9
    i32x4.neg
    i32x4.extract_lane 3)
  (func (export "min_s") (result i32)
    v128.const i32x4 -2147483648 -1 5 11
    v128.const i32x4 1 2 6 -12
    i32x4.min_s
    i32x4.extract_lane 0)
  (func (export "min_u") (result i32)
    v128.const i32x4 -2147483648 -1 5 11
    v128.const i32x4 1 2 6 -12
    i32x4.min_u
    i32x4.extract_lane 0)
  (func (export "max_s") (result i32)
    v128.const i32x4 -2147483648 -1 5 11
    v128.const i32x4 1 2 6 -12
    i32x4.max_s
    i32x4.extract_lane 0)
  (func (export "max_u") (result i32)
    v128.const i32x4 -2147483648 -1 5 11
    v128.const i32x4 1 2 6 -12
    i32x4.max_u
    i32x4.extract_lane 0)
  (func (export "dot_wrap") (result i32)
    v128.const i16x8 -32768 -32768 32767 32767 -3 4 -1 -2
    v128.const i16x8 -32768 -32768 1 1 5 -6 -7 8
    i32x4.dot_i16x8_s
    i32x4.extract_lane 0)
  (func (export "dot_regular") (result i32)
    v128.const i16x8 -32768 -32768 32767 32767 -3 4 -1 -2
    v128.const i16x8 -32768 -32768 1 1 5 -6 -7 8
    i32x4.dot_i16x8_s
    i32x4.extract_lane 2))"#;

const EXPORTS: [(&str, i32); 8] = [
    ("abs_min", i32::MIN),
    ("neg_3", -9),
    ("min_s", i32::MIN),
    ("min_u", 1),
    ("max_s", 1),
    ("max_u", i32::MIN),
    ("dot_wrap", i32::MIN),
    ("dot_regular", -39),
];

fn mini(instance: &mut MiniInstance, name: &str) -> i32 {
    match instance
        .invoke_export_values(name, &[])
        .expect("mini i32x4 ALU execution")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected mini result for {name}: {other:?}"),
    }
}

#[test]
fn i32x4_remaining_alu_matches_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("i32x4 ALU WAT parses");
    let module = parse_module(&bytes).expect("mini parses i32x4 ALU fixture");
    let mut mini_instance = MiniInstance::new(module).expect("mini instantiates i32x4 ALU fixture");

    let mut config = Config::new();
    config.wasm_simd(true);
    let engine = Engine::new(&config).expect("SIMD engine");
    let module = ReferenceModule::new(&engine, &bytes).expect("Wasmtime compiles i32x4 ALU fixture");
    let mut store = Store::new(&engine, ());
    let reference =
        ReferenceInstance::new(&mut store, &module, &[]).expect("Wasmtime instantiates");

    for (name, expected) in EXPORTS {
        let mini_value = mini(&mut mini_instance, name);
        let reference_value = reference
            .get_typed_func::<(), i32>(&mut store, name)
            .expect("i32x4 ALU signature")
            .call(&mut store, ())
            .expect("Wasmtime i32x4 ALU execution");
        assert_eq!(mini_value, expected, "mini {name} drifted");
        assert_eq!(reference_value, expected, "Wasmtime {name} drifted");
        assert_eq!(mini_value, reference_value, "{name} differential mismatch");
    }
}
