use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, RuntimeError, Value};
use wasmtime::{
    Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store, Trap as ReferenceTrap,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableTrapClass {
    TableOutOfBounds,
    IndirectCallToNull,
    BadSignature,
}

fn normalize_mini_table_error(error: RuntimeError) -> TableTrapClass {
    match error {
        RuntimeError::TableElementOutOfBounds(_) => TableTrapClass::TableOutOfBounds,
        RuntimeError::UninitializedTableElement(_) => TableTrapClass::IndirectCallToNull,
        RuntimeError::IndirectCallTypeMismatch { .. } => TableTrapClass::BadSignature,
        other => panic!("unmapped mini-runtime table differential error: {other:?}"),
    }
}

fn normalize_reference_table_error(error: &wasmtime::Error) -> TableTrapClass {
    let trap = error
        .downcast_ref::<ReferenceTrap>()
        .unwrap_or_else(|| panic!("Wasmtime execution error was not a trap: {error:?}"));
    match *trap {
        ReferenceTrap::TableOutOfBounds => TableTrapClass::TableOutOfBounds,
        ReferenceTrap::IndirectCallToNull => TableTrapClass::IndirectCallToNull,
        ReferenceTrap::BadSignature => TableTrapClass::BadSignature,
        other => panic!("unmapped Wasmtime table differential trap: {other:?}"),
    }
}

fn run_mini_table_trap(bytes: &[u8]) -> TableTrapClass {
    let module =
        parse_module(bytes).expect("table differential fixture must parse in mini runtime");
    let mut instance =
        MiniInstance::new(module).expect("table differential fixture must validate/instantiate");
    let error = instance
        .invoke_export_values("run", &[])
        .expect_err("table differential fixture must trap at execution");
    normalize_mini_table_error(error)
}

fn run_reference_table_trap(engine: &Engine, bytes: &[u8]) -> TableTrapClass {
    let module =
        ReferenceModule::new(engine, bytes).expect("table fixture must compile in Wasmtime");
    let mut store = Store::new(engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("table fixture must instantiate in Wasmtime");
    let run = instance
        .get_typed_func::<(), i32>(&mut store, "run")
        .expect("table fixture run export must be [] -> [i32]");
    let error = run
        .call(&mut store, ())
        .expect_err("table differential fixture must trap in Wasmtime");
    normalize_reference_table_error(&error)
}

#[test]
fn table_and_indirect_call_trap_classes_match_wasmtime() {
    let cases = [
        (
            "uninitialized table element",
            r#"(module
                (type $unary (func (param i32) (result i32)))
                (table 2 funcref)
                (func $target (type $unary)
                    local.get 0
                    i32.const 1
                    i32.add)
                (elem (i32.const 0) $target)
                (func (export "run") (result i32)
                    i32.const 41
                    i32.const 1
                    call_indirect (type $unary)))"#,
            TableTrapClass::IndirectCallToNull,
        ),
        (
            "table element out of bounds",
            r#"(module
                (type $unary (func (param i32) (result i32)))
                (table 2 funcref)
                (func $target (type $unary)
                    local.get 0
                    i32.const 1
                    i32.add)
                (elem (i32.const 0) $target)
                (func (export "run") (result i32)
                    i32.const 41
                    i32.const 2
                    call_indirect (type $unary)))"#,
            TableTrapClass::TableOutOfBounds,
        ),
        (
            "indirect call signature mismatch",
            r#"(module
                (type $unary (func (param i32) (result i32)))
                (type $binary (func (param i32 i32) (result i32)))
                (table 1 funcref)
                (func $target (type $unary)
                    local.get 0
                    i32.const 1
                    i32.add)
                (elem (i32.const 0) $target)
                (func (export "run") (result i32)
                    i32.const 20
                    i32.const 22
                    i32.const 0
                    call_indirect (type $binary)))"#,
            TableTrapClass::BadSignature,
        ),
    ];

    let engine = Engine::default();
    for (name, wat, expected) in cases {
        let bytes = wat::parse_str(wat)
            .unwrap_or_else(|error| panic!("failed to compile table WAT for {name}: {error}"));
        let mini = run_mini_table_trap(&bytes);
        let reference = run_reference_table_trap(&engine, &bytes);
        assert_eq!(mini, expected, "mini runtime trap mismatch for {name}");
        assert_eq!(reference, expected, "Wasmtime trap mismatch for {name}");
        assert_eq!(mini, reference, "table differential mismatch for {name}");
    }
}

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

fn run_mini_i32_sequence(bytes: &[u8], calls: usize) -> Vec<i32> {
    let module =
        parse_module(bytes).expect("stateful differential fixture must parse in mini runtime");
    let mut instance =
        MiniInstance::new(module).expect("stateful differential fixture must validate/instantiate");
    (0..calls)
        .map(|call| {
            match instance
                .invoke_export_values("run", &[])
                .unwrap_or_else(|error| panic!("mini stateful call {call} trapped: {error:?}"))
                .as_slice()
            {
                [Value::I32(value)] => *value,
                other => panic!("mini stateful call {call} returned unexpected values: {other:?}"),
            }
        })
        .collect()
}

fn run_reference_i32_sequence(engine: &Engine, bytes: &[u8], calls: usize) -> Vec<i32> {
    let module =
        ReferenceModule::new(engine, bytes).expect("stateful fixture must compile in Wasmtime");
    let mut store = Store::new(engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("stateful fixture must instantiate in Wasmtime");
    let run = instance
        .get_typed_func::<(), i32>(&mut store, "run")
        .expect("stateful run export must be [] -> [i32]");
    (0..calls)
        .map(|call| {
            run.call(&mut store, ())
                .unwrap_or_else(|error| panic!("Wasmtime stateful call {call} trapped: {error:?}"))
        })
        .collect()
}

#[test]
fn generated_stateful_global_memory_sequences_match_wasmtime() {
    const SEED: u64 = 0x3c6e_f372_fe94_f82b;
    const CALLS: usize = 4;
    let mut rng = XorShift64::new(SEED);
    let engine = Engine::default();

    for case in 0..64 {
        let initial = rng.next_i32();
        let step = rng.next_i32();
        let address = (rng.next_u64() % 65_533) as u32;
        let wat = format!(
            "(module
                (memory 1)
                (global $g (mut i32) (i32.const {initial}))
                (func (export \"run\") (result i32)
                    global.get $g
                    i32.const {step}
                    i32.add
                    global.set $g
                    i32.const {address}
                    i32.const {address}
                    i32.load
                    global.get $g
                    i32.add
                    i32.store
                    i32.const {address}
                    i32.load))"
        );
        let bytes = wat::parse_str(&wat).unwrap_or_else(|error| {
            panic!("generated stateful WAT failed at seed={SEED:#018x} case={case}: {error}")
        });

        let mut global = initial;
        let mut memory = 0_i32;
        let mut expected = Vec::with_capacity(CALLS);
        for _ in 0..CALLS {
            global = global.wrapping_add(step);
            memory = memory.wrapping_add(global);
            expected.push(memory);
        }

        let mini = run_mini_i32_sequence(&bytes, CALLS);
        let reference = run_reference_i32_sequence(&engine, &bytes, CALLS);
        assert_eq!(
            mini, expected,
            "mini stateful mismatch at seed={SEED:#018x} case={case}: initial={initial}, step={step}, address={address}"
        );
        assert_eq!(
            reference, expected,
            "Wasmtime stateful mismatch at seed={SEED:#018x} case={case}: initial={initial}, step={step}, address={address}"
        );
        assert_eq!(
            mini, reference,
            "stateful differential mismatch at seed={SEED:#018x} case={case}: initial={initial}, step={step}, address={address}"
        );
    }
}
