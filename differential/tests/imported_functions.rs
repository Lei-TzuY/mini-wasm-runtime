use std::sync::{Arc, Mutex};

use wasm_parser::{parse_module, ValueType};
use wasm_runtime::{HostCapabilities, HostRegistry, Instance as MiniInstance, Value};
use wasmtime::{
    Engine, Extern, Func, Instance as ReferenceInstance, Module as ReferenceModule, Store,
};

struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        assert_ne!(seed, 0);
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        let mut value = self.state;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.state = value;
        value
    }

    fn next_i64(&mut self) -> i64 {
        self.next_u64() as i64
    }
}

fn stateful_host_wat(salt: i64) -> String {
    format!(
        "(module
            (import \"env\" \"host\" (func $host (param i64) (result i64)))
            (func (export \"run\") (param i64) (result i64)
                local.get 0
                call $host
                i64.const {salt}
                i64.xor))"
    )
}

fn make_mini_stateful(bytes: &[u8], state: Arc<Mutex<i64>>) -> MiniInstance {
    let callback_state = Arc::clone(&state);
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "host",
            vec![ValueType::I64],
            vec![ValueType::I64],
            HostCapabilities::NONE,
            move |_ctx, args| {
                let mut value = callback_state
                    .lock()
                    .expect("mini host-state mutex poisoned");
                *value = value.wrapping_add(args[0].as_i64());
                Ok(Some(Value::I64(*value)))
            },
        )
        .expect("register mini imported host function");
    let module = parse_module(bytes).expect("imported-function fixture must parse in mini runtime");
    MiniInstance::with_hosts(module, hosts)
        .expect("imported-function fixture must validate and instantiate in mini runtime")
}

fn make_reference_stateful(
    engine: &Engine,
    bytes: &[u8],
    state: Arc<Mutex<i64>>,
) -> (Store<()>, ReferenceInstance) {
    let module =
        ReferenceModule::new(engine, bytes).expect("imported-function fixture must compile");
    let mut store = Store::new(engine, ());
    let callback_state = Arc::clone(&state);
    let host = Func::wrap(&mut store, move |input: i64| -> i64 {
        let mut value = callback_state
            .lock()
            .expect("Wasmtime host-state mutex poisoned");
        *value = value.wrapping_add(input);
        *value
    });
    let instance = ReferenceInstance::new(&mut store, &module, &[Extern::Func(host)])
        .expect("instantiate Wasmtime imported-function fixture");
    (store, instance)
}

#[test]
fn generated_stateful_imported_functions_match_wasmtime() {
    const SEED: u64 = 0x1f83_d9ab_fb41_bd6b;
    const CALLS: usize = 5;
    let mut rng = XorShift64::new(SEED);
    let engine = Engine::default();

    for case in 0..48 {
        let initial_state = rng.next_i64();
        let salt = rng.next_i64();
        let inputs: Vec<i64> = (0..CALLS).map(|_| rng.next_i64()).collect();
        let wat = stateful_host_wat(salt);
        let bytes = wat::parse_str(&wat).unwrap_or_else(|error| {
            panic!(
                "generated imported-function WAT failed at seed={SEED:#018x} case={case}: {error}"
            )
        });

        let mini_state = Arc::new(Mutex::new(initial_state));
        let reference_state = Arc::new(Mutex::new(initial_state));
        let mut mini = make_mini_stateful(&bytes, Arc::clone(&mini_state));
        let (mut store, reference) =
            make_reference_stateful(&engine, &bytes, Arc::clone(&reference_state));
        let reference_run = reference
            .get_typed_func::<i64, i64>(&mut store, "run")
            .expect("Wasmtime run export must be [i64] -> [i64]");

        let mut expected_state = initial_state;
        for (call, input) in inputs.into_iter().enumerate() {
            expected_state = expected_state.wrapping_add(input);
            let expected = expected_state ^ salt;

            let mini_result = mini
                .invoke_export_values("run", &[Value::I64(input)])
                .unwrap_or_else(|error| {
                    panic!("mini imported-function call trapped at seed={SEED:#018x} case={case} call={call}: {error:?}")
                });
            let mini_value = match mini_result.as_slice() {
                [Value::I64(value)] => *value,
                other => panic!("unexpected mini imported-function result shape: {other:?}"),
            };
            let reference_value = reference_run.call(&mut store, input).unwrap_or_else(|error| {
                panic!("Wasmtime imported-function call trapped at seed={SEED:#018x} case={case} call={call}: {error:?}")
            });

            assert_eq!(
                mini_value, expected,
                "mini mismatch at case={case} call={call}"
            );
            assert_eq!(
                reference_value, expected,
                "Wasmtime mismatch at case={case} call={call}"
            );
            assert_eq!(
                mini_value, reference_value,
                "imported-function differential mismatch at case={case} call={call}"
            );
            assert_eq!(
                *mini_state.lock().expect("read mini host state"),
                expected_state
            );
            assert_eq!(
                *reference_state.lock().expect("read Wasmtime host state"),
                expected_state
            );
        }
    }
}

#[test]
fn imported_host_state_is_shared_across_two_instances() {
    let salt = 0x1357_9bdf_2468_ace0_i64;
    let bytes = wat::parse_str(stateful_host_wat(salt)).expect("compile shared-host WAT");

    let mini_state = Arc::new(Mutex::new(11_i64));
    let mut mini_first = make_mini_stateful(&bytes, Arc::clone(&mini_state));
    let mut mini_second = make_mini_stateful(&bytes, Arc::clone(&mini_state));

    let engine = Engine::default();
    let module =
        ReferenceModule::new(&engine, &bytes).expect("compile Wasmtime shared-host module");
    let mut store = Store::new(&engine, ());
    let reference_state = Arc::new(Mutex::new(11_i64));
    let callback_state = Arc::clone(&reference_state);
    let host = Func::wrap(&mut store, move |input: i64| -> i64 {
        let mut value = callback_state
            .lock()
            .expect("Wasmtime shared host-state mutex poisoned");
        *value = value.wrapping_add(input);
        *value
    });
    let imports = [Extern::Func(host)];
    let first = ReferenceInstance::new(&mut store, &module, &imports).unwrap();
    let second = ReferenceInstance::new(&mut store, &module, &imports).unwrap();
    let first_run = first.get_typed_func::<i64, i64>(&mut store, "run").unwrap();
    let second_run = second
        .get_typed_func::<i64, i64>(&mut store, "run")
        .unwrap();

    let inputs = [3_i64, -7, 19, i64::MAX, 5];
    let mut expected_state = 11_i64;
    for (call, input) in inputs.into_iter().enumerate() {
        expected_state = expected_state.wrapping_add(input);
        let expected = expected_state ^ salt;

        let mini_instance = if call % 2 == 0 {
            &mut mini_first
        } else {
            &mut mini_second
        };
        let mini_result = mini_instance
            .invoke_export_values("run", &[Value::I64(input)])
            .unwrap();
        let mini_value = match mini_result.as_slice() {
            [Value::I64(value)] => *value,
            other => panic!("unexpected shared mini host result shape: {other:?}"),
        };
        let reference_value = if call % 2 == 0 {
            first_run.call(&mut store, input).unwrap()
        } else {
            second_run.call(&mut store, input).unwrap()
        };

        assert_eq!(mini_value, expected);
        assert_eq!(reference_value, expected);
        assert_eq!(mini_value, reference_value);
    }

    assert_eq!(*mini_state.lock().unwrap(), expected_state);
    assert_eq!(*reference_state.lock().unwrap(), expected_state);
}

fn mixed_host_expected(i32_value: i32, i64_value: i64, f32_value: f32, f64_value: f64) -> i64 {
    let mut value = i64::from(i32_value)
        .wrapping_mul(31)
        .wrapping_add(i64_value);
    value = value.rotate_left(7) ^ i64::from(f32_value.to_bits());
    value.rotate_left(11) ^ f64_value.to_bits() as i64
}

#[test]
fn mixed_numeric_imported_function_parameters_match_wasmtime() {
    let wat = r#"(module
        (import "env" "host" (func $host (param i32 i64 f32 f64) (result i64)))
        (func (export "run") (param i32 i64 f32 f64) (result i64)
            local.get 0
            local.get 1
            local.get 2
            local.get 3
            call $host))"#;
    let bytes = wat::parse_str(wat).expect("compile mixed imported-function WAT");
    let args = (-17_i32, 0x1122_3344_5566_7788_i64, -0.0_f32, -2.25_f64);
    let expected = mixed_host_expected(args.0, args.1, args.2, args.3);

    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "host",
            vec![
                ValueType::I32,
                ValueType::I64,
                ValueType::F32,
                ValueType::F64,
            ],
            vec![ValueType::I64],
            HostCapabilities::NONE,
            |_ctx, values| {
                Ok(Some(Value::I64(mixed_host_expected(
                    values[0].as_i32(),
                    values[1].as_i64(),
                    values[2].as_f32(),
                    values[3].as_f64(),
                ))))
            },
        )
        .unwrap();
    let module = parse_module(&bytes).unwrap();
    let mut mini = MiniInstance::with_hosts(module, hosts).unwrap();
    let mini_result = mini
        .invoke_export_values(
            "run",
            &[
                Value::I32(args.0),
                Value::I64(args.1),
                Value::F32(args.2),
                Value::F64(args.3),
            ],
        )
        .unwrap();
    let mini_value = match mini_result.as_slice() {
        [Value::I64(value)] => *value,
        other => panic!("unexpected mixed mini host result shape: {other:?}"),
    };

    let engine = Engine::default();
    let module = ReferenceModule::new(&engine, &bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let host = Func::wrap(&mut store, |a: i32, b: i64, c: f32, d: f64| -> i64 {
        mixed_host_expected(a, b, c, d)
    });
    let instance = ReferenceInstance::new(&mut store, &module, &[Extern::Func(host)]).unwrap();
    let run = instance
        .get_typed_func::<(i32, i64, f32, f64), i64>(&mut store, "run")
        .unwrap();
    let reference_value = run.call(&mut store, args).unwrap();

    assert_eq!(mini_value, expected);
    assert_eq!(reference_value, expected);
    assert_eq!(mini_value, reference_value);
}
