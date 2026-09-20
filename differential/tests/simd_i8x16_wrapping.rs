use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"(module
  (func (export "add_pos_wrap") (result i32)
    v128.const i8x16 127 -128 -1 0 1 2 3 4 5 6 7 8 9 10 11 12
    v128.const i8x16 1 -1 1 -1 127 126 125 124 123 122 121 120 119 118 117 116
    i8x16.add
    i8x16.extract_lane_s 0)
  (func (export "add_neg_wrap") (result i32)
    v128.const i8x16 127 -128 -1 0 1 2 3 4 5 6 7 8 9 10 11 12
    v128.const i8x16 1 -1 1 -1 127 126 125 124 123 122 121 120 119 118 117 116
    i8x16.add
    i8x16.extract_lane_s 1)
  (func (export "sub_neg_wrap") (result i32)
    v128.const i8x16 -128 0 127 -1 10 20 30 40 50 60 70 80 90 100 110 120
    v128.const i8x16 1 1 -1 1 20 30 40 50 60 70 80 90 100 110 120 127
    i8x16.sub
    i8x16.extract_lane_s 0)
  (func (export "sub_zero_wrap") (result i32)
    v128.const i8x16 -128 0 127 -1 10 20 30 40 50 60 70 80 90 100 110 120
    v128.const i8x16 1 1 -1 1 20 30 40 50 60 70 80 90 100 110 120 127
    i8x16.sub
    i8x16.extract_lane_u 1))"#;

const EXPORTS: [(&str, i32); 4] = [
    ("add_pos_wrap", -128),
    ("add_neg_wrap", 127),
    ("sub_neg_wrap", 127),
    ("sub_zero_wrap", 255),
];

#[test]
fn i8x16_wrapping_add_sub_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("i8x16 wrapping WAT parses");
    let module = parse_module(&bytes).expect("mini parses i8x16 wrapping fixture");
    let mut mini = MiniInstance::new(module).expect("mini instantiates i8x16 wrapping fixture");

    let mut config = Config::new();
    config.wasm_simd(true);
    let engine = Engine::new(&config).expect("SIMD engine");
    let module =
        ReferenceModule::new(&engine, &bytes).expect("Wasmtime compiles i8x16 wrapping fixture");
    let mut store = Store::new(&engine, ());
    let reference =
        ReferenceInstance::new(&mut store, &module, &[]).expect("Wasmtime instantiates");

    for (name, expected) in EXPORTS {
        let mini_value = match mini
            .invoke_export_values(name, &[])
            .expect("mini i8x16 wrapping execution")
            .as_slice()
        {
            [Value::I32(value)] => *value,
            other => panic!("unexpected mini wrapping result for {name}: {other:?}"),
        };
        let reference_value = reference
            .get_typed_func::<(), i32>(&mut store, name)
            .expect("i8x16 wrapping signature")
            .call(&mut store, ())
            .expect("Wasmtime i8x16 wrapping execution");
        assert_eq!(mini_value, expected, "mini {name} drifted");
        assert_eq!(reference_value, expected, "Wasmtime {name} drifted");
        assert_eq!(mini_value, reference_value, "{name} differential mismatch");
    }
}
