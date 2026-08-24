use std::{error::Error, fmt};

use wasm_parser::{parse_module, ValueType};
use wasm_runtime::{
    HostCapabilities, HostError, HostRegistry, Instance as MiniInstance, RuntimeError, Value,
};
use wasmtime::{
    Engine, Error as ReferenceError, Extern, Func, Instance as ReferenceInstance,
    Module as ReferenceModule, Store,
};

const OOB_ADDRESS: u32 = 65_535;
const OOB_WIDTH: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailureClass {
    CapabilityDenied(&'static str),
    MemoryUnavailable,
    MemoryOutOfBounds { address: u64, width: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostOutcome {
    Value(i32),
    Failure(FailureClass),
}

#[derive(Debug, Clone, Copy)]
enum Operation {
    ReadWithoutCapability,
    WriteWithoutCapability,
    ReadWithoutMemory,
    ReadOutOfBounds,
}

#[derive(Debug, Clone, Copy)]
struct Scenario {
    name: &'static str,
    operation: Operation,
    capabilities: HostCapabilities,
    has_memory: bool,
    failure: FailureClass,
}

#[derive(Debug)]
struct ReferenceHostFailure(FailureClass);

impl fmt::Display for ReferenceHostFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("typed host failure")
    }
}

impl Error for ReferenceHostFailure {}

fn host_failure_wat(has_memory: bool) -> &'static str {
    if has_memory {
        r#"(module
            (import "env" "gate" (func $gate (param i32) (result i32)))
            (memory 1)
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
    } else {
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
}

fn make_mini(bytes: &[u8], scenario: Scenario) -> MiniInstance {
    let operation = scenario.operation;
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "gate",
            vec![ValueType::I32],
            vec![ValueType::I32],
            scenario.capabilities,
            move |ctx, args| {
                let input = args[0].as_i32();
                if input == 0 {
                    match operation {
                        Operation::ReadWithoutCapability => {
                            let _ = ctx.read_memory(0, 1)?;
                        }
                        Operation::WriteWithoutCapability => {
                            ctx.write_memory(0, &[0])?;
                        }
                        Operation::ReadWithoutMemory => {
                            let _ = ctx.memory_size_pages()?;
                        }
                        Operation::ReadOutOfBounds => {
                            let _ = ctx.read_memory(OOB_ADDRESS, OOB_WIDTH)?;
                        }
                    }
                    panic!("typed host-failure operation unexpectedly succeeded");
                }
                Ok(Some(Value::I32(input.wrapping_add(10))))
            },
        )
        .expect("register typed host-failure callback");
    MiniInstance::with_hosts(
        parse_module(bytes).expect("typed host-failure fixture must parse in mini runtime"),
        hosts,
    )
    .expect("typed host-failure fixture must validate and instantiate in mini runtime")
}

fn make_reference(
    engine: &Engine,
    bytes: &[u8],
    failure: FailureClass,
) -> (Store<()>, ReferenceInstance) {
    let module =
        ReferenceModule::new(engine, bytes).expect("typed host-failure fixture must compile");
    let mut store = Store::new(engine, ());
    let gate = Func::wrap(&mut store, move |input: i32| -> wasmtime::Result<i32> {
        if input == 0 {
            return Err(ReferenceError::new(ReferenceHostFailure(failure)));
        }
        Ok(input.wrapping_add(10))
    });
    let instance = ReferenceInstance::new(&mut store, &module, &[Extern::Func(gate)])
        .expect("instantiate Wasmtime typed host-failure fixture");
    (store, instance)
}

fn normalize_mini(result: Result<Vec<Value>, RuntimeError>) -> HostOutcome {
    match result {
        Ok(values) => match values.as_slice() {
            [Value::I32(value)] => HostOutcome::Value(*value),
            other => panic!("unexpected mini typed host-failure result shape: {other:?}"),
        },
        Err(RuntimeError::HostCallFailed { error, .. }) => {
            let failure = match error {
                HostError::CapabilityDenied(capability) => FailureClass::CapabilityDenied(capability),
                HostError::MemoryUnavailable => FailureClass::MemoryUnavailable,
                HostError::MemoryOutOfBounds { address, width } => {
                    FailureClass::MemoryOutOfBounds { address, width }
                }
                HostError::Message(message) => {
                    panic!("unexpected generic mini host message in typed class test: {message:?}")
                }
            };
            HostOutcome::Failure(failure)
        }
        Err(error) => panic!("unmapped mini typed host failure: {error:?}"),
    }
}

fn normalize_reference(result: Result<i32, ReferenceError>) -> HostOutcome {
    match result {
        Ok(value) => HostOutcome::Value(value),
        Err(error) => {
            let failure = error
                .downcast_ref::<ReferenceHostFailure>()
                .unwrap_or_else(|| panic!("unmapped Wasmtime typed host failure: {error:?}"));
            HostOutcome::Failure(failure.0)
        }
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
fn structured_host_failures_normalize_by_type_and_recover_like_wasmtime() {
    let scenarios = [
        Scenario {
            name: "read-capability-denied",
            operation: Operation::ReadWithoutCapability,
            capabilities: HostCapabilities::NONE,
            has_memory: false,
            failure: FailureClass::CapabilityDenied("memory.read"),
        },
        Scenario {
            name: "write-capability-denied",
            operation: Operation::WriteWithoutCapability,
            capabilities: HostCapabilities::MEMORY_READ,
            has_memory: false,
            failure: FailureClass::CapabilityDenied("memory.write"),
        },
        Scenario {
            name: "memory-unavailable",
            operation: Operation::ReadWithoutMemory,
            capabilities: HostCapabilities::MEMORY_READ,
            has_memory: false,
            failure: FailureClass::MemoryUnavailable,
        },
        Scenario {
            name: "memory-out-of-bounds",
            operation: Operation::ReadOutOfBounds,
            capabilities: HostCapabilities::MEMORY_READ,
            has_memory: true,
            failure: FailureClass::MemoryOutOfBounds {
                address: u64::from(OOB_ADDRESS),
                width: OOB_WIDTH,
            },
        },
    ];

    let engine = Engine::default();
    for scenario in scenarios {
        let bytes = wat::parse_str(host_failure_wat(scenario.has_memory))
            .expect("compile typed host-failure WAT");
        let mut mini = make_mini(&bytes, scenario);
        let (mut store, reference) = make_reference(&engine, &bytes, scenario.failure);
        let reference_run = reference
            .get_typed_func::<i32, i32>(&mut store, "run")
            .expect("Wasmtime run export must be [i32] -> [i32]");
        let reference_guest_successes = reference
            .get_typed_func::<(), i32>(&mut store, "guest_successes")
            .expect("Wasmtime guest_successes export must be [] -> [i32]");

        let mut expected_guest_successes = 0_i32;
        for (step, input) in [0_i32, 7, 0, -9].into_iter().enumerate() {
            let expected = if input == 0 {
                HostOutcome::Failure(scenario.failure)
            } else {
                expected_guest_successes = expected_guest_successes.wrapping_add(1);
                HostOutcome::Value(
                    input
                        .wrapping_add(10)
                        .wrapping_add(expected_guest_successes),
                )
            };

            let mini_outcome = normalize_mini(
                mini.invoke_export_values("run", &[Value::I32(input)]),
            );
            let reference_outcome = normalize_reference(reference_run.call(&mut store, input));

            assert_eq!(
                mini_outcome, expected,
                "mini typed host-failure mismatch for {} at step={step}",
                scenario.name
            );
            assert_eq!(
                reference_outcome, expected,
                "Wasmtime typed host-failure mismatch for {} at step={step}",
                scenario.name
            );
            assert_eq!(
                mini_outcome, reference_outcome,
                "typed host-failure differential mismatch for {} at step={step}",
                scenario.name
            );
            assert_eq!(
                mini_guest_successes(&mut mini),
                expected_guest_successes,
                "mini guest state advanced across typed host failure for {} at step={step}",
                scenario.name
            );
            assert_eq!(
                reference_guest_successes.call(&mut store, ()).unwrap(),
                expected_guest_successes,
                "Wasmtime guest state advanced across typed host failure for {} at step={step}",
                scenario.name
            );
        }
    }
}
