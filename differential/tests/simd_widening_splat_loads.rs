use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const MEMORY32_FIXTURE: &str = r#"
(module
  (memory 1)
  (data (i32.const 0) "\80\7f\ff\01\00\aa\55\fe\00\80\ff\7f\ff\ff\34\12\00\00\00\80\ff\ff\ff\7f\80\34\12\78\56\34\12\08\07\06\05\04\03\02\01")
  (func (export "l8s") (result i32) i32.const 0 v128.load8x8_s i16x8.extract_lane_s 0)
  (func (export "l8u") (result i32) i32.const 0 v128.load8x8_u i16x8.extract_lane_u 0)
  (func (export "l16s") (result i32) i32.const 8 v128.load16x4_s i32x4.extract_lane 0)
  (func (export "l16u") (result i32) i32.const 8 v128.load16x4_u i32x4.extract_lane 0)
  (func (export "l32s") (result i64) i32.const 16 v128.load32x2_s i64x2.extract_lane 0)
  (func (export "l32u") (result i64) i32.const 16 v128.load32x2_u i64x2.extract_lane 0)
  (func (export "s8") (result i32) i32.const 24 v128.load8_splat i8x16.extract_lane_u 15)
  (func (export "s16") (result i32) i32.const 25 v128.load16_splat i16x8.extract_lane_u 7)
  (func (export "s32") (result i32) i32.const 27 v128.load32_splat i32x4.extract_lane 3)
  (func (export "s64") (result i64) i32.const 31 v128.load64_splat i64x2.extract_lane 1))
"#;

const MEMORY64_FIXTURE: &str = r#"
(module
  (memory i64 1)
  (func (export "load32x2_u") (result i64)
    i64.const 0
    i32.const -1
    i32.store
    i64.const 0
    v128.load32x2_u
    i64x2.extract_lane 0))
"#;

fn mini_i32(instance: &mut MiniInstance, export: &str) -> i32 {
    match instance
        .invoke_export_values(export, &[])
        .unwrap_or_else(|error| panic!("mini {export} failed: {error}"))
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected mini i32 result for {export}: {other:?}"),
    }
}

fn mini_i64(instance: &mut MiniInstance, export: &str) -> i64 {
    match instance
        .invoke_export_values(export, &[])
        .unwrap_or_else(|error| panic!("mini {export} failed: {error}"))
        .as_slice()
    {
        [Value::I64(value)] => *value,
        other => panic!("unexpected mini i64 result for {export}: {other:?}"),
    }
}

fn reference_engine(memory64: bool) -> Engine {
    let mut config = Config::new();
    config.wasm_simd(true);
    if memory64 {
        config.wasm_memory64(true);
    }
    Engine::new(&config).expect("reference engine")
}

#[test]
fn widening_and_splat_loads_match_wasmtime_reference() {
    let bytes = wat::parse_str(MEMORY32_FIXTURE).expect("memory32 fixture parses");
    let module = parse_module(&bytes).expect("mini parses memory32 fixture");
    let mut mini = MiniInstance::new(module).expect("mini instantiates memory32 fixture");

    let expected_i32 = [
        ("l8s", -128),
        ("l8u", 128),
        ("l16s", -32_768),
        ("l16u", 32_768),
        ("s8", 128),
        ("s16", 0x1234),
        ("s32", 0x1234_5678),
    ];
    let expected_i64 = [
        ("l32s", -2_147_483_648),
        ("l32u", 2_147_483_648),
        ("s64", 0x0102_0304_0506_0708),
    ];

    let engine = reference_engine(false);
    let reference_module = ReferenceModule::new(&engine, &bytes).expect("Wasmtime compiles");
    let mut store = Store::new(&engine, ());
    let reference =
        ReferenceInstance::new(&mut store, &reference_module, &[]).expect("Wasmtime instantiates");

    for (name, expected) in expected_i32 {
        let mini_value = mini_i32(&mut mini, name);
        let reference_value = reference
            .get_typed_func::<(), i32>(&mut store, name)
            .expect("i32 signature")
            .call(&mut store, ())
            .expect("Wasmtime execution");
        assert_eq!(mini_value, expected, "mini {name} drifted");
        assert_eq!(reference_value, expected, "Wasmtime {name} drifted");
        assert_eq!(mini_value, reference_value, "{name} differential mismatch");
    }

    for (name, expected) in expected_i64 {
        let mini_value = mini_i64(&mut mini, name);
        let reference_value = reference
            .get_typed_func::<(), i64>(&mut store, name)
            .expect("i64 signature")
            .call(&mut store, ())
            .expect("Wasmtime execution");
        assert_eq!(mini_value, expected, "mini {name} drifted");
        assert_eq!(reference_value, expected, "Wasmtime {name} drifted");
        assert_eq!(mini_value, reference_value, "{name} differential mismatch");
    }
}

#[test]
fn memory64_widening_load_matches_wasmtime_reference() {
    let bytes = wat::parse_str(MEMORY64_FIXTURE).expect("memory64 fixture parses");
    let module = parse_module(&bytes).expect("mini parses memory64 fixture");
    let mut mini = MiniInstance::new(module).expect("mini instantiates memory64 fixture");
    let mini_value = mini_i64(&mut mini, "load32x2_u");

    let engine = reference_engine(true);
    let reference_module =
        ReferenceModule::new(&engine, &bytes).expect("Wasmtime compiles memory64 fixture");
    let mut store = Store::new(&engine, ());
    let reference =
        ReferenceInstance::new(&mut store, &reference_module, &[]).expect("Wasmtime instantiates");
    let reference_value = reference
        .get_typed_func::<(), i64>(&mut store, "load32x2_u")
        .expect("memory64 signature")
        .call(&mut store, ())
        .expect("Wasmtime memory64 execution");

    assert_eq!(mini_value, 4_294_967_295);
    assert_eq!(reference_value, 4_294_967_295);
    assert_eq!(mini_value, reference_value);
}
