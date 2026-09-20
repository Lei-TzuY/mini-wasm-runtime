use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"(module
  (func (export "i16_low_s") (result i32)
    v128.const i8x16 -128 127 -2 3 4 5 6 7 8 -9 10 -11 12 -13 14 -15
    v128.const i8x16 2 2 -3 4 5 6 7 8 -2 3 -4 5 -6 7 -8 9
    i16x8.extmul_low_i8x16_s
    i16x8.extract_lane_s 0)
  (func (export "i16_high_s") (result i32)
    v128.const i8x16 -128 127 -2 3 4 5 6 7 8 -9 10 -11 12 -13 14 -15
    v128.const i8x16 2 2 -3 4 5 6 7 8 -2 3 -4 5 -6 7 -8 9
    i16x8.extmul_high_i8x16_s
    i16x8.extract_lane_s 1)
  (func (export "i16_low_u") (result i32)
    v128.const i8x16 -128 127 -2 3 4 5 6 7 8 -9 10 -11 12 -13 14 -15
    v128.const i8x16 2 2 -3 4 5 6 7 8 -2 3 -4 5 -6 7 -8 9
    i16x8.extmul_low_i8x16_u
    i16x8.extract_lane_u 0)
  (func (export "i16_high_u") (result i32)
    v128.const i8x16 -128 127 -2 3 4 5 6 7 8 -9 10 -11 12 -13 14 -15
    v128.const i8x16 2 2 -3 4 5 6 7 8 -2 3 -4 5 -6 7 -8 9
    i16x8.extmul_high_i8x16_u
    i16x8.extract_lane_u 7)
  (func (export "i32_low_s") (result i32)
    v128.const i16x8 -32768 32767 -2 3 4 -5 6 -7
    v128.const i16x8 2 2 -3 4 -5 6 -7 8
    i32x4.extmul_low_i16x8_s
    i32x4.extract_lane 0)
  (func (export "i32_high_s") (result i32)
    v128.const i16x8 -32768 32767 -2 3 4 -5 6 -7
    v128.const i16x8 2 2 -3 4 -5 6 -7 8
    i32x4.extmul_high_i16x8_s
    i32x4.extract_lane 1)
  (func (export "i32_low_u") (result i32)
    v128.const i16x8 -32768 32767 -2 3 4 -5 6 -7
    v128.const i16x8 2 2 -3 4 -5 6 -7 8
    i32x4.extmul_low_i16x8_u
    i32x4.extract_lane 0)
  (func (export "i32_high_u") (result i32)
    v128.const i16x8 -32768 32767 -2 3 4 -5 6 -7
    v128.const i16x8 2 2 -3 4 -5 6 -7 8
    i32x4.extmul_high_i16x8_u
    i32x4.extract_lane 3))"#;

const EXPORTS: [(&str, i32); 8] = [
    ("i16_low_s", -256),
    ("i16_high_s", -27),
    ("i16_low_u", 256),
    ("i16_high_u", 2169),
    ("i32_low_s", -65_536),
    ("i32_high_s", -30),
    ("i32_low_u", 65_536),
    ("i32_high_u", 524_232),
];

fn mini(instance: &mut MiniInstance, name: &str) -> i32 {
    match instance
        .invoke_export_values(name, &[])
        .expect("mini extmul execution")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected mini result for {name}: {other:?}"),
    }
}

#[test]
fn cross_width_extmul_matches_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("extmul WAT parses");
    let module = parse_module(&bytes).expect("mini parses extmul fixture");
    let mut mini_instance = MiniInstance::new(module).expect("mini instantiates extmul fixture");

    let mut config = Config::new();
    config.wasm_simd(true);
    let engine = Engine::new(&config).expect("SIMD engine");
    let module = ReferenceModule::new(&engine, &bytes).expect("Wasmtime compiles extmul fixture");
    let mut store = Store::new(&engine, ());
    let reference =
        ReferenceInstance::new(&mut store, &module, &[]).expect("Wasmtime instantiates");

    for (name, expected) in EXPORTS {
        let mini_value = mini(&mut mini_instance, name);
        let reference_value = reference
            .get_typed_func::<(), i32>(&mut store, name)
            .expect("extmul signature")
            .call(&mut store, ())
            .expect("Wasmtime extmul execution");
        assert_eq!(mini_value, expected, "mini {name} drifted");
        assert_eq!(reference_value, expected, "Wasmtime {name} drifted");
        assert_eq!(mini_value, reference_value, "{name} differential mismatch");
    }
}
