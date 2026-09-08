use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const MEMORY32_FIXTURE: &str = r#"
(module
  (memory 1)
  (func (export "round_trip") (result i32)
    i32.const 8
    v128.const i32x4 1 2 287454020 4
    v128.store
    i32.const 8
    v128.load
    i32x4.extract_lane 2)
  (func (export "oob_load") (result i32)
    i32.const 65530
    v128.load
    i32x4.extract_lane 0)
  (func (export "oob_store")
    i32.const 65530
    v128.const i32x4 -1591662960 -1524290924 -1456918888 -1389546852
    v128.store)
  (func (export "tail") (result i32)
    i32.const 65532
    i32.load))
"#;

const MEMORY64_FIXTURE: &str = r#"
(module
  (memory i64 1)
  (func (export "round_trip") (result i32)
    i64.const 16
    v128.const i32x4 5 6 1432778632 8
    v128.store
    i64.const 16
    v128.load
    i32x4.extract_lane 2)
  (func (export "wide_offset") (result i32)
    i64.const 0
    v128.load offset=4294967296
    i32x4.extract_lane 0))
"#;

fn mini_i32(instance: &mut MiniInstance, export: &str) -> i32 {
    let values = instance
        .invoke_export_values(export, &[])
        .unwrap_or_else(|error| panic!("mini {export} must succeed: {error}"));
    match values.as_slice() {
        [Value::I32(value)] => *value,
        other => panic!("unexpected mini result for {export}: {other:?}"),
    }
}

fn mini_memory32_trace(bytes: &[u8]) -> (i32, bool, bool, i32) {
    let module = parse_module(bytes).expect("mini runtime must parse memory32 SIMD fixture");
    let mut instance =
        MiniInstance::new(module).expect("mini runtime must instantiate SIMD fixture");
    let round_trip = mini_i32(&mut instance, "round_trip");
    let oob_load_traps = instance.invoke_export_values("oob_load", &[]).is_err();
    let oob_store_traps = instance.invoke_export_values("oob_store", &[]).is_err();
    let tail = mini_i32(&mut instance, "tail");
    (round_trip, oob_load_traps, oob_store_traps, tail)
}

fn mini_memory64_trace(bytes: &[u8]) -> (i32, bool) {
    let module = parse_module(bytes).expect("mini runtime must parse memory64 SIMD fixture");
    let mut instance =
        MiniInstance::new(module).expect("mini runtime must instantiate memory64 SIMD fixture");
    let round_trip = mini_i32(&mut instance, "round_trip");
    let wide_offset_traps = instance.invoke_export_values("wide_offset", &[]).is_err();
    (round_trip, wide_offset_traps)
}

fn reference_engine(memory64: bool) -> Engine {
    let mut config = Config::new();
    config.wasm_simd(true);
    if memory64 {
        config.wasm_memory64(true);
    }
    Engine::new(&config).expect("SIMD reference engine must initialize")
}

fn reference_memory32_trace(bytes: &[u8]) -> (i32, bool, bool, i32) {
    let engine = reference_engine(false);
    let module = ReferenceModule::new(&engine, bytes).expect("Wasmtime must compile SIMD memory32");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate SIMD memory32");

    let round_trip = instance
        .get_typed_func::<(), i32>(&mut store, "round_trip")
        .expect("round_trip must be [] -> [i32]")
        .call(&mut store, ())
        .expect("Wasmtime SIMD round trip must succeed");
    let oob_load_traps = instance
        .get_typed_func::<(), i32>(&mut store, "oob_load")
        .expect("oob_load must be [] -> [i32]")
        .call(&mut store, ())
        .is_err();
    let oob_store_traps = instance
        .get_typed_func::<(), ()>(&mut store, "oob_store")
        .expect("oob_store must be [] -> []")
        .call(&mut store, ())
        .is_err();
    let tail = instance
        .get_typed_func::<(), i32>(&mut store, "tail")
        .expect("tail must be [] -> [i32]")
        .call(&mut store, ())
        .expect("tail observation must succeed");
    (round_trip, oob_load_traps, oob_store_traps, tail)
}

fn reference_memory64_trace(bytes: &[u8]) -> (i32, bool) {
    let engine = reference_engine(true);
    let module = ReferenceModule::new(&engine, bytes).expect("Wasmtime must compile SIMD memory64");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate SIMD memory64");

    let round_trip = instance
        .get_typed_func::<(), i32>(&mut store, "round_trip")
        .expect("round_trip must be [] -> [i32]")
        .call(&mut store, ())
        .expect("Wasmtime memory64 SIMD round trip must succeed");
    let wide_offset_traps = instance
        .get_typed_func::<(), i32>(&mut store, "wide_offset")
        .expect("wide_offset must be [] -> [i32]")
        .call(&mut store, ())
        .is_err();
    (round_trip, wide_offset_traps)
}

#[test]
fn v128_memory_semantics_match_wasmtime_reference() {
    let memory32 = wat::parse_str(MEMORY32_FIXTURE).expect("memory32 SIMD WAT must parse");
    let memory64 = wat::parse_str(MEMORY64_FIXTURE).expect("memory64 SIMD WAT must parse");

    let expected32 = (0x1122_3344, true, true, 0);
    let mini32 = mini_memory32_trace(&memory32);
    let reference32 = reference_memory32_trace(&memory32);
    assert_eq!(mini32, expected32, "mini memory32 SIMD trace drifted");
    assert_eq!(
        reference32, expected32,
        "Wasmtime memory32 SIMD trace drifted"
    );
    assert_eq!(mini32, reference32, "memory32 SIMD traces diverged");

    let expected64 = (0x5566_7788, true);
    let mini64 = mini_memory64_trace(&memory64);
    let reference64 = reference_memory64_trace(&memory64);
    assert_eq!(mini64, expected64, "mini memory64 SIMD trace drifted");
    assert_eq!(
        reference64, expected64,
        "Wasmtime memory64 SIMD trace drifted"
    );
    assert_eq!(mini64, reference64, "memory64 SIMD traces diverged");
}
