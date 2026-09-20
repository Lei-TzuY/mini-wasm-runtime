use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"(module
  (func (export "i16_s0") (result i32)
    v128.const i8x16 -128 -128 127 127 -5 -6 10 20 -1 1 100 -100 50 60 -70 80
    i16x8.extadd_pairwise_i8x16_s
    i16x8.extract_lane_s 0)
  (func (export "i16_s2") (result i32)
    v128.const i8x16 -128 -128 127 127 -5 -6 10 20 -1 1 100 -100 50 60 -70 80
    i16x8.extadd_pairwise_i8x16_s
    i16x8.extract_lane_s 2)
  (func (export "i16_u0") (result i32)
    v128.const i8x16 -128 -128 127 127 -5 -6 10 20 -1 1 100 -100 50 60 -70 80
    i16x8.extadd_pairwise_i8x16_u
    i16x8.extract_lane_u 0)
  (func (export "i16_u2") (result i32)
    v128.const i8x16 -128 -128 127 127 -5 -6 10 20 -1 1 100 -100 50 60 -70 80
    i16x8.extadd_pairwise_i8x16_u
    i16x8.extract_lane_u 2)
  (func (export "i32_s0") (result i32)
    v128.const i16x8 -32768 -32768 32767 32767 -3000 4000 -1 -2
    i32x4.extadd_pairwise_i16x8_s
    i32x4.extract_lane 0)
  (func (export "i32_s2") (result i32)
    v128.const i16x8 -32768 -32768 32767 32767 -3000 4000 -1 -2
    i32x4.extadd_pairwise_i16x8_s
    i32x4.extract_lane 2)
  (func (export "i32_u0") (result i32)
    v128.const i16x8 -32768 -32768 32767 32767 -3000 4000 -1 -2
    i32x4.extadd_pairwise_i16x8_u
    i32x4.extract_lane 0)
  (func (export "i32_u2") (result i32)
    v128.const i16x8 -32768 -32768 32767 32767 -3000 4000 -1 -2
    i32x4.extadd_pairwise_i16x8_u
    i32x4.extract_lane 2))"#;

const EXPORTS: [(&str, i32); 8] = [
    ("i16_s0", -256),
    ("i16_s2", -11),
    ("i16_u0", 256),
    ("i16_u2", 501),
    ("i32_s0", -65_536),
    ("i32_s2", 1_000),
    ("i32_u0", 65_536),
    ("i32_u2", 66_536),
];

fn mini(instance: &mut MiniInstance, name: &str) -> i32 {
    match instance
        .invoke_export_values(name, &[])
        .expect("mini pairwise execution")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected mini result for {name}: {other:?}"),
    }
}

#[test]
fn extended_pairwise_addition_matches_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("pairwise WAT parses");
    let module = parse_module(&bytes).expect("mini parses pairwise fixture");
    let mut mini_instance = MiniInstance::new(module).expect("mini instantiates pairwise fixture");

    let mut config = Config::new();
    config.wasm_simd(true);
    let engine = Engine::new(&config).expect("SIMD engine");
    let module = ReferenceModule::new(&engine, &bytes).expect("Wasmtime compiles pairwise fixture");
    let mut store = Store::new(&engine, ());
    let reference =
        ReferenceInstance::new(&mut store, &module, &[]).expect("Wasmtime instantiates");

    for (name, expected) in EXPORTS {
        let mini_value = mini(&mut mini_instance, name);
        let reference_value = reference
            .get_typed_func::<(), i32>(&mut store, name)
            .expect("pairwise signature")
            .call(&mut store, ())
            .expect("Wasmtime pairwise execution");
        assert_eq!(mini_value, expected, "mini {name} drifted");
        assert_eq!(reference_value, expected, "Wasmtime {name} drifted");
        assert_eq!(mini_value, reference_value, "{name} differential mismatch");
    }
}
