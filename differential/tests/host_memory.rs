use wasm_parser::{parse_module, ValueType};
use wasm_runtime::{
    HostCapabilities, HostError, HostRegistry, Instance as MiniInstance, RuntimeError, Value,
};
use wasmtime::{Caller, Engine, Extern, Func, Instance as ReferenceInstance, Module, Store};

fn host_memory_wat() -> &'static str {
    r#"(module
        (import "env" "update" (func $update (param i32 i32) (result i32)))
        (memory (export "memory") 1 1)
        (func (export "run") (param $address i32) (param $delta i32) (result i32 i32)
            local.get $address
            local.get $delta
            call $update
            local.get $address
            i32.load)
        (func (export "peek") (param $address i32) (result i32)
            local.get $address
            i32.load))"#
}

fn make_mini(bytes: &[u8], capabilities: HostCapabilities) -> MiniInstance {
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "update",
            vec![ValueType::I32, ValueType::I32],
            vec![ValueType::I32],
            capabilities,
            |ctx, args| {
                let address = args[0].as_i32() as u32;
                let delta = args[1].as_i32();
                let bytes = ctx.read_memory(address, 4)?;
                let current = i32::from_le_bytes(
                    bytes
                        .try_into()
                        .expect("four-byte host-memory differential read"),
                );
                let next = current.wrapping_add(delta);
                ctx.write_memory(address, &next.to_le_bytes())?;
                Ok(Some(Value::I32(next)))
            },
        )
        .expect("register mini host-memory callback");
    MiniInstance::with_hosts(
        parse_module(bytes).expect("host-memory fixture must parse in mini runtime"),
        hosts,
    )
    .expect("host-memory fixture must validate and instantiate in mini runtime")
}

fn make_reference(engine: &Engine, bytes: &[u8]) -> (Store<()>, ReferenceInstance) {
    let module = Module::new(engine, bytes).expect("host-memory fixture must compile in Wasmtime");
    let mut store = Store::new(engine, ());
    let update = Func::wrap(
        &mut store,
        |mut caller: Caller<'_, ()>, address: i32, delta: i32| -> i32 {
            let memory = match caller.get_export("memory") {
                Some(Extern::Memory(memory)) => memory,
                other => panic!("missing Wasmtime memory export in host callback: {other:?}"),
            };
            let address = address as u32 as usize;
            let mut bytes = [0_u8; 4];
            memory
                .read(&caller, address, &mut bytes)
                .expect("Wasmtime host callback read must stay in bounds");
            let next = i32::from_le_bytes(bytes).wrapping_add(delta);
            memory
                .write(&mut caller, address, &next.to_le_bytes())
                .expect("Wasmtime host callback write must stay in bounds");
            next
        },
    );
    let instance = ReferenceInstance::new(&mut store, &module, &[Extern::Func(update)])
        .expect("instantiate Wasmtime host-memory fixture");
    (store, instance)
}

#[test]
fn host_memory_read_write_state_matches_wasmtime() {
    const SEED: u64 = 0xa54f_f53a_5f1d_36f1;
    const SLOTS: usize = 16;
    let bytes = wat::parse_str(host_memory_wat()).expect("compile host-memory WAT");
    let mut mini = make_mini(&bytes, HostCapabilities::MEMORY_READ_WRITE);
    let engine = Engine::default();
    let (mut store, reference) = make_reference(&engine, &bytes);
    let reference_run = reference
        .get_typed_func::<(i32, i32), (i32, i32)>(&mut store, "run")
        .expect("Wasmtime run export must be [i32, i32] -> [i32, i32]");

    let mut expected_slots = [0_i32; SLOTS];
    let mut state = SEED;
    for case in 0..96 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let slot = (state as usize) % SLOTS;
        let address = (slot * 4) as i32;
        let delta = (state >> 32) as u32 as i32;
        expected_slots[slot] = expected_slots[slot].wrapping_add(delta);
        let expected = expected_slots[slot];

        let mini_result = mini
            .invoke_export_values("run", &[Value::I32(address), Value::I32(delta)])
            .unwrap_or_else(|error| {
                panic!("mini host-memory call failed at seed={SEED:#018x} case={case}: {error:?}")
            });
        let mini_pair = match mini_result.as_slice() {
            [Value::I32(callback), Value::I32(guest_load)] => (*callback, *guest_load),
            other => panic!("unexpected mini host-memory result shape: {other:?}"),
        };
        let reference_pair = reference_run
            .call(&mut store, (address, delta))
            .unwrap_or_else(|error| {
                panic!("Wasmtime host-memory call failed at seed={SEED:#018x} case={case}: {error:?}")
            });

        assert_eq!(mini_pair, (expected, expected), "mini mismatch at case={case}");
        assert_eq!(
            reference_pair,
            (expected, expected),
            "Wasmtime mismatch at case={case}"
        );
        assert_eq!(mini_pair, reference_pair, "differential mismatch at case={case}");
    }
}

fn assert_peek_zero(instance: &mut MiniInstance) {
    assert_eq!(
        instance
            .invoke_export_values("peek", &[Value::I32(0)])
            .expect("peek after denied host access must succeed"),
        vec![Value::I32(0)]
    );
}

#[test]
fn mini_host_memory_capabilities_fail_closed() {
    let bytes = wat::parse_str(host_memory_wat()).expect("compile host-memory WAT");

    let mut no_capabilities = make_mini(&bytes, HostCapabilities::NONE);
    assert!(matches!(
        no_capabilities.invoke_export_values("run", &[Value::I32(0), Value::I32(7)]),
        Err(RuntimeError::HostCallFailed {
            error: HostError::CapabilityDenied("memory.read"),
            ..
        })
    ));
    assert_peek_zero(&mut no_capabilities);

    let mut read_only = make_mini(&bytes, HostCapabilities::MEMORY_READ);
    assert!(matches!(
        read_only.invoke_export_values("run", &[Value::I32(0), Value::I32(7)]),
        Err(RuntimeError::HostCallFailed {
            error: HostError::CapabilityDenied("memory.write"),
            ..
        })
    ));
    assert_peek_zero(&mut read_only);
}

#[test]
fn mini_host_memory_out_of_bounds_fails_without_partial_write() {
    let bytes = wat::parse_str(host_memory_wat()).expect("compile host-memory WAT");
    let mut mini = make_mini(&bytes, HostCapabilities::MEMORY_READ_WRITE);
    assert!(matches!(
        mini.invoke_export_values("run", &[Value::I32(65_534), Value::I32(1)]),
        Err(RuntimeError::HostCallFailed {
            error: HostError::MemoryOutOfBounds {
                address: 65_534,
                width: 4,
            },
            ..
        })
    ));
    assert_peek_zero(&mut mini);
}
