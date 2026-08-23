use std::{
    error::Error,
    fmt,
    sync::{Arc, Mutex},
};

use wasm_parser::{parse_module, ValueType};
use wasm_runtime::{
    HostCapabilities, HostError, HostRegistry, Instance as MiniInstance, RuntimeError, Value,
};
use wasmtime::{
    Engine, Error as ReferenceError, Extern, Func, Instance as ReferenceInstance,
    Module as ReferenceModule, Store,
};

const MINI_SENTINEL_MESSAGE: &str = "phase6 sentinel host rejection";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostOutcome {
    Value(i32),
    CallbackRejected,
}

#[derive(Debug)]
struct SentinelHostFailure {
    input: i32,
}

impl fmt::Display for SentinelHostFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "sentinel host rejected input {}", self.input)
    }
}

impl Error for SentinelHostFailure {}

fn should_reject(input: i32) -> bool {
    input & 0x7 == 0
}

fn host_failure_wat() -> &'static str {
    r#"(module
        (import "env" "gate" (func $gate (param i32) (result i32)))
        (global $guest_successes (mut i32) (i32.const 0))
        (func (export "run") (param $input i32) (result i32)
            local.get $input
            call $gate
            global.get $guest_successes
            i32.const 1
            i32.add
            global.set $guest_successes
            global.get $guest_successes
            i32.add)
        (func (export "guest_successes") (result i32)
            global.get $guest_successes))"#
}

fn make_mini(bytes: &[u8], host_calls: Arc<Mutex<i32>>) -> MiniInstance {
    let callback_calls = Arc::clone(&host_calls);
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "gate",
            vec![ValueType::I32],
            vec![ValueType::I32],
            HostCapabilities::NONE,
            move |_ctx, args| {
                let input = args[0].as_i32();
                let mut calls = callback_calls
                    .lock()
                    .expect("mini host-call counter mutex poisoned");
                *calls = calls.wrapping_add(1);
                if should_reject(input) {
                    return Err(HostError::message(MINI_SENTINEL_MESSAGE));
                }
                Ok(Some(Value::I32(input.wrapping_add(*calls))))
            },
        )
        .expect("register mini sentinel host callback");
    MiniInstance::with_hosts(
        parse_module(bytes).expect("host-failure fixture must parse in mini runtime"),
        hosts,
    )
    .expect("host-failure fixture must validate and instantiate in mini runtime")
}

fn make_reference(
    engine: &Engine,
    bytes: &[u8],
    host_calls: Arc<Mutex<i32>>,
) -> (Store<()>, ReferenceInstance) {
    let module = ReferenceModule::new(engine, bytes).expect("host-failure fixture must compile");
    let mut store = Store::new(engine, ());
    let callback_calls = Arc::clone(&host_calls);
    let gate = Func::wrap(&mut store, move |input: i32| -> wasmtime::Result<i32> {
        let mut calls = callback_calls
            .lock()
            .expect("Wasmtime host-call counter mutex poisoned");
        *calls = calls.wrapping_add(1);
        if should_reject(input) {
            return Err(ReferenceError::new(SentinelHostFailure { input }));
        }
        Ok(input.wrapping_add(*calls))
    });
    let instance = ReferenceInstance::new(&mut store, &module, &[Extern::Func(gate)])
        .expect("instantiate Wasmtime host-failure fixture");
    (store, instance)
}

fn normalize_mini(result: Result<Vec<Value>, RuntimeError>) -> HostOutcome {
    match result {
        Ok(values) => match values.as_slice() {
            [Value::I32(value)] => HostOutcome::Value(*value),
            other => panic!("unexpected mini host-failure result shape: {other:?}"),
        },
        Err(RuntimeError::HostCallFailed {
            error: HostError::Message(message),
            ..
        }) if message == MINI_SENTINEL_MESSAGE => HostOutcome::CallbackRejected,
        Err(error) => panic!("unmapped mini host failure: {error:?}"),
    }
}

fn normalize_reference(result: Result<i32, ReferenceError>) -> HostOutcome {
    match result {
        Ok(value) => HostOutcome::Value(value),
        Err(error) if error.downcast_ref::<SentinelHostFailure>().is_some() => {
            HostOutcome::CallbackRejected
        }
        Err(error) => panic!("unmapped Wasmtime host failure: {error:?}"),
    }
}

fn mini_guest_successes(instance: &mut MiniInstance) -> i32 {
    match instance
        .invoke_export_values("guest_successes", &[])
        .expect("mini guest-success counter export must execute")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected mini guest-success counter shape: {other:?}"),
    }
}

#[test]
fn imported_callback_failures_normalize_and_recover_like_wasmtime() {
    const SEED: u64 = 0x5be0_cd19_137e_2179;
    const CASES: i32 = 96;

    let bytes = wat::parse_str(host_failure_wat()).expect("compile host-failure WAT");
    let mini_host_calls = Arc::new(Mutex::new(0_i32));
    let reference_host_calls = Arc::new(Mutex::new(0_i32));
    let mut mini = make_mini(&bytes, Arc::clone(&mini_host_calls));
    let engine = Engine::default();
    let (mut store, reference) = make_reference(&engine, &bytes, Arc::clone(&reference_host_calls));
    let reference_run = reference
        .get_typed_func::<i32, i32>(&mut store, "run")
        .expect("Wasmtime run export must be [i32] -> [i32]");
    let reference_guest_successes = reference
        .get_typed_func::<(), i32>(&mut store, "guest_successes")
        .expect("Wasmtime guest_successes export must be [] -> [i32]");

    let mut rng = SEED;
    let mut expected_host_calls = 0_i32;
    let mut expected_guest_successes = 0_i32;
    let mut rejected = 0_i32;

    for case in 0..CASES {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let raw = rng as u32 as i32;
        let input = if case % 5 == 0 { raw & !0x7 } else { raw | 1 };

        expected_host_calls = expected_host_calls.wrapping_add(1);
        let expected = if should_reject(input) {
            rejected += 1;
            HostOutcome::CallbackRejected
        } else {
            expected_guest_successes = expected_guest_successes.wrapping_add(1);
            HostOutcome::Value(
                input
                    .wrapping_add(expected_host_calls)
                    .wrapping_add(expected_guest_successes),
            )
        };

        let mini_outcome = normalize_mini(mini.invoke_export_values("run", &[Value::I32(input)]));
        let reference_outcome = normalize_reference(reference_run.call(&mut store, input));

        assert_eq!(mini_outcome, expected, "mini mismatch at case={case}");
        assert_eq!(
            reference_outcome, expected,
            "Wasmtime mismatch at case={case}"
        );
        assert_eq!(
            mini_outcome, reference_outcome,
            "host-failure differential mismatch at case={case}"
        );
        assert_eq!(
            *mini_host_calls.lock().expect("read mini host-call counter"),
            expected_host_calls,
            "mini host-side failure state mismatch at case={case}"
        );
        assert_eq!(
            *reference_host_calls
                .lock()
                .expect("read Wasmtime host-call counter"),
            expected_host_calls,
            "Wasmtime host-side failure state mismatch at case={case}"
        );
        assert_eq!(
            mini_guest_successes(&mut mini),
            expected_guest_successes,
            "mini guest state advanced across a failed host call at case={case}"
        );
        assert_eq!(
            reference_guest_successes.call(&mut store, ()).unwrap(),
            expected_guest_successes,
            "Wasmtime guest state advanced across a failed host call at case={case}"
        );
    }

    assert!(
        rejected > 0,
        "deterministic corpus must exercise host failures"
    );
    assert!(
        rejected < CASES,
        "deterministic corpus must also exercise recovery after successful host calls"
    );
}
