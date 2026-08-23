use wasm_parser::parse_module;
use wasm_runtime::{GlobalHandle, HostRegistry, Instance as MiniInstance, MemoryHandle, Value};
use wasmtime::{
    Engine, Extern, Global, GlobalType, Instance as ReferenceInstance, Memory, MemoryType,
    Module as ReferenceModule, Mutability, Store, Val, ValType,
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

    fn next_i32(&mut self) -> i32 {
        self.next_u64() as u32 as i32
    }
}

fn imported_state_wat(address: u32) -> String {
    format!(
        "(module
            (import \"env\" \"g\" (global $g (mut i32)))
            (import \"env\" \"mem\" (memory 1 2))
            (func (export \"run\") (param $bump i32) (result i32 i32)
                global.get $g
                local.get $bump
                i32.add
                global.set $g
                i32.const {address}
                i32.const {address}
                i32.load
                global.get $g
                i32.add
                i32.store
                global.get $g
                i32.const {address}
                i32.load))"
    )
}

fn mini_imports(
    bytes: &[u8],
    initial_global: i32,
    address: u32,
    initial_memory: i32,
) -> (MiniInstance, GlobalHandle, MemoryHandle) {
    let global = GlobalHandle::mutable(Value::I32(initial_global));
    let memory = MemoryHandle::new(1, Some(2)).expect("create imported mini memory");
    memory
        .write(address, &initial_memory.to_le_bytes())
        .expect("seed imported mini memory");
    let mut hosts = HostRegistry::new();
    hosts
        .register_global("env", "g", global.clone())
        .expect("register imported mini global");
    hosts
        .register_memory("env", "mem", memory.clone())
        .expect("register imported mini memory");
    let module = parse_module(bytes).expect("imported-state fixture must parse in mini runtime");
    let instance = MiniInstance::with_hosts(module, hosts)
        .expect("imported-state fixture must validate and instantiate in mini runtime");
    (instance, global, memory)
}

fn read_mini_i32(memory: &MemoryHandle, address: u32) -> i32 {
    let bytes = memory
        .read(address, 4)
        .expect("read imported mini memory after execution");
    i32::from_le_bytes(bytes.try_into().expect("four-byte mini memory read"))
}

struct ReferenceImports {
    store: Store<()>,
    instance: ReferenceInstance,
    global: Global,
    memory: Memory,
}

fn reference_imports(
    engine: &Engine,
    bytes: &[u8],
    initial_global: i32,
    address: u32,
    initial_memory: i32,
) -> ReferenceImports {
    let module = ReferenceModule::new(engine, bytes)
        .expect("imported-state fixture must compile in Wasmtime");
    let mut store = Store::new(engine, ());
    let global = Global::new(
        &mut store,
        GlobalType::new(ValType::I32, Mutability::Var),
        Val::I32(initial_global),
    )
    .expect("create Wasmtime imported global");
    let memory = Memory::new(&mut store, MemoryType::new(1, Some(2)))
        .expect("create Wasmtime imported memory");
    memory
        .write(&mut store, address as usize, &initial_memory.to_le_bytes())
        .expect("seed Wasmtime imported memory");
    let imports = [Extern::Global(global), Extern::Memory(memory)];
    let instance = ReferenceInstance::new(&mut store, &module, &imports)
        .expect("instantiate Wasmtime imported-state fixture");
    ReferenceImports {
        store,
        instance,
        global,
        memory,
    }
}

fn read_reference_i32(memory: Memory, store: &Store<()>, address: u32) -> i32 {
    let mut bytes = [0_u8; 4];
    memory
        .read(store, address as usize, &mut bytes)
        .expect("read Wasmtime imported memory after execution");
    i32::from_le_bytes(bytes)
}

fn reference_global_i32(global: Global, store: &mut Store<()>) -> i32 {
    match global.get(store) {
        Val::I32(value) => value,
        other => panic!("unexpected Wasmtime imported global value: {other:?}"),
    }
}

#[test]
fn generated_imported_global_memory_state_matches_wasmtime() {
    const SEED: u64 = 0x9b05_688c_2b3e_6c1f;
    const CALLS: usize = 5;
    let mut rng = XorShift64::new(SEED);
    let engine = Engine::default();

    for case in 0..48 {
        let address = (rng.next_u64() % 65_533) as u32;
        let initial_global = rng.next_i32();
        let initial_memory = rng.next_i32();
        let override_global = rng.next_i32();
        let override_memory = rng.next_i32();
        let bumps: Vec<i32> = (0..CALLS).map(|_| rng.next_i32()).collect();
        let wat = imported_state_wat(address);
        let bytes = wat::parse_str(&wat).unwrap_or_else(|error| {
            panic!("generated imported-state WAT failed at seed={SEED:#018x} case={case}: {error}")
        });

        let (mut mini, mini_global, mini_memory) =
            mini_imports(&bytes, initial_global, address, initial_memory);
        let mut reference =
            reference_imports(&engine, &bytes, initial_global, address, initial_memory);
        let reference_run = reference
            .instance
            .get_typed_func::<i32, (i32, i32)>(&mut reference.store, "run")
            .expect("Wasmtime imported-state run export must be [i32] -> [i32, i32]");

        let mut expected_global = initial_global;
        let mut expected_memory = initial_memory;
        for (call, bump) in bumps.iter().copied().enumerate() {
            if call == 2 {
                expected_global = override_global;
                expected_memory = override_memory;
                mini_global
                    .set(Value::I32(override_global))
                    .expect("override mini imported global");
                mini_memory
                    .write(address, &override_memory.to_le_bytes())
                    .expect("override mini imported memory");
                reference
                    .global
                    .set(&mut reference.store, Val::I32(override_global))
                    .expect("override Wasmtime imported global");
                reference
                    .memory
                    .write(
                        &mut reference.store,
                        address as usize,
                        &override_memory.to_le_bytes(),
                    )
                    .expect("override Wasmtime imported memory");
            }

            expected_global = expected_global.wrapping_add(bump);
            expected_memory = expected_memory.wrapping_add(expected_global);
            let expected = (expected_global, expected_memory);

            let mini_result = mini
                .invoke_export_values("run", &[Value::I32(bump)])
                .unwrap_or_else(|error| {
                    panic!("mini imported-state call trapped at seed={SEED:#018x} case={case} call={call}: {error:?}")
                });
            let mini_pair = match mini_result.as_slice() {
                [Value::I32(global), Value::I32(memory)] => (*global, *memory),
                other => panic!("unexpected mini imported-state result shape: {other:?}"),
            };
            let reference_pair = reference_run
                .call(&mut reference.store, bump)
                .unwrap_or_else(|error| {
                    panic!("Wasmtime imported-state call trapped at seed={SEED:#018x} case={case} call={call}: {error:?}")
                });

            assert_eq!(
                mini_pair, expected,
                "mini result mismatch at case={case} call={call}"
            );
            assert_eq!(
                reference_pair, expected,
                "Wasmtime result mismatch at case={case} call={call}"
            );
            assert_eq!(
                mini_pair, reference_pair,
                "differential result mismatch at case={case} call={call}"
            );
            assert_eq!(mini_global.get(), Value::I32(expected_global));
            assert_eq!(read_mini_i32(&mini_memory, address), expected_memory);
            assert_eq!(
                reference_global_i32(reference.global, &mut reference.store),
                expected_global
            );
            assert_eq!(
                read_reference_i32(reference.memory, &reference.store, address),
                expected_memory
            );
        }
    }
}

#[test]
fn imported_global_and_memory_are_shared_across_two_instances() {
    let address = 64_u32;
    let wat = imported_state_wat(address);
    let bytes = wat::parse_str(&wat).expect("compile two-instance imported-state WAT");

    let mini_global = GlobalHandle::mutable(Value::I32(10));
    let mini_memory = MemoryHandle::new(1, Some(2)).expect("create shared mini memory");
    mini_memory
        .write(address, &100_i32.to_le_bytes())
        .expect("seed shared mini memory");
    let make_mini = || {
        let mut hosts = HostRegistry::new();
        hosts
            .register_global("env", "g", mini_global.clone())
            .unwrap();
        hosts
            .register_memory("env", "mem", mini_memory.clone())
            .unwrap();
        MiniInstance::with_hosts(parse_module(&bytes).unwrap(), hosts).unwrap()
    };
    let mut mini_first = make_mini();
    let mut mini_second = make_mini();

    let engine = Engine::default();
    let module = ReferenceModule::new(&engine, &bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let global = Global::new(
        &mut store,
        GlobalType::new(ValType::I32, Mutability::Var),
        Val::I32(10),
    )
    .unwrap();
    let memory = Memory::new(&mut store, MemoryType::new(1, Some(2))).unwrap();
    memory
        .write(&mut store, address as usize, &100_i32.to_le_bytes())
        .unwrap();
    let imports = [Extern::Global(global), Extern::Memory(memory)];
    let first = ReferenceInstance::new(&mut store, &module, &imports).unwrap();
    let second = ReferenceInstance::new(&mut store, &module, &imports).unwrap();
    let first_run = first
        .get_typed_func::<i32, (i32, i32)>(&mut store, "run")
        .unwrap();
    let second_run = second
        .get_typed_func::<i32, (i32, i32)>(&mut store, "run")
        .unwrap();

    let bumps = [1_i32, 5, -3, 9];
    let mut expected_global = 10_i32;
    let mut expected_memory = 100_i32;
    for (call, bump) in bumps.into_iter().enumerate() {
        expected_global = expected_global.wrapping_add(bump);
        expected_memory = expected_memory.wrapping_add(expected_global);
        let expected = (expected_global, expected_memory);

        let mini_instance = if call % 2 == 0 {
            &mut mini_first
        } else {
            &mut mini_second
        };
        let mini_result = mini_instance
            .invoke_export_values("run", &[Value::I32(bump)])
            .unwrap();
        let mini_pair = match mini_result.as_slice() {
            [Value::I32(global), Value::I32(memory)] => (*global, *memory),
            other => panic!("unexpected shared mini result shape: {other:?}"),
        };
        let reference_pair = if call % 2 == 0 {
            first_run.call(&mut store, bump).unwrap()
        } else {
            second_run.call(&mut store, bump).unwrap()
        };

        assert_eq!(mini_pair, expected);
        assert_eq!(reference_pair, expected);
        assert_eq!(mini_pair, reference_pair);
    }

    assert_eq!(mini_global.get(), Value::I32(expected_global));
    assert_eq!(read_mini_i32(&mini_memory, address), expected_memory);
    assert_eq!(reference_global_i32(global, &mut store), expected_global);
    assert_eq!(read_reference_i32(memory, &store, address), expected_memory);
}
