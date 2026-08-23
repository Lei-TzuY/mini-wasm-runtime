use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

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

    fn next_i64(&mut self) -> i64 {
        self.next_u64() as i64
    }
}

fn run_mini_i32_sequence(bytes: &[u8], calls: usize) -> Vec<i32> {
    let module = parse_module(bytes).expect("generated table fixture must parse in mini runtime");
    let mut instance =
        MiniInstance::new(module).expect("generated table fixture must validate/instantiate");
    (0..calls)
        .map(|call| {
            match instance
                .invoke_export_values("run", &[])
                .unwrap_or_else(|error| panic!("mini table call {call} trapped: {error:?}"))
                .as_slice()
            {
                [Value::I32(value)] => *value,
                other => panic!("mini table call {call} returned unexpected values: {other:?}"),
            }
        })
        .collect()
}

fn run_reference_i32_sequence(engine: &Engine, bytes: &[u8], calls: usize) -> Vec<i32> {
    let module = ReferenceModule::new(engine, bytes)
        .expect("generated table fixture must compile in Wasmtime");
    let mut store = Store::new(engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("generated table fixture must instantiate in Wasmtime");
    let run = instance
        .get_typed_func::<(), i32>(&mut store, "run")
        .expect("generated table run export must be [] -> [i32]");
    (0..calls)
        .map(|call| {
            run.call(&mut store, ())
                .unwrap_or_else(|error| panic!("Wasmtime table call {call} trapped: {error:?}"))
        })
        .collect()
}

#[test]
fn generated_table_dispatch_state_sequences_match_wasmtime() {
    const SEED: u64 = 0xa54f_f53a_5f1d_36f1;
    const CALLS: usize = 6;
    let mut rng = XorShift64::new(SEED);
    let engine = Engine::default();

    for case in 0..64 {
        let first = rng.next_i32();
        let second = rng.next_i32();
        let wat = format!(
            "(module
                (type $ret (func (result i32)))
                (table 2 funcref)
                (global $index (mut i32) (i32.const 0))
                (func $first (type $ret) (result i32)
                    i32.const {first})
                (func $second (type $ret) (result i32)
                    i32.const {second})
                (elem (i32.const 0) $first $second)
                (func (export \"run\") (result i32)
                    global.get $index
                    global.get $index
                    i32.const 1
                    i32.xor
                    global.set $index
                    call_indirect (type $ret)))"
        );
        let bytes = wat::parse_str(&wat).unwrap_or_else(|error| {
            panic!("generated table WAT failed at seed={SEED:#018x} case={case}: {error}")
        });
        let expected = vec![first, second, first, second, first, second];
        let mini = run_mini_i32_sequence(&bytes, CALLS);
        let reference = run_reference_i32_sequence(&engine, &bytes, CALLS);

        assert_eq!(
            mini, expected,
            "mini table sequence mismatch at seed={SEED:#018x} case={case}: first={first}, second={second}"
        );
        assert_eq!(
            reference, expected,
            "Wasmtime table sequence mismatch at seed={SEED:#018x} case={case}: first={first}, second={second}"
        );
        assert_eq!(
            mini, reference,
            "table sequence differential mismatch at seed={SEED:#018x} case={case}: first={first}, second={second}"
        );
    }
}

fn run_mini_pair(bytes: &[u8]) -> (i32, i64) {
    let module = parse_module(bytes).expect("multi-value fixture must parse in mini runtime");
    let mut instance =
        MiniInstance::new(module).expect("multi-value fixture must validate/instantiate");
    match instance
        .invoke_export_values("run", &[])
        .expect("multi-value mini invocation must not trap")
        .as_slice()
    {
        [Value::I32(first), Value::I64(second)] => (*first, *second),
        other => panic!("unexpected mini multi-value result shape: {other:?}"),
    }
}

fn run_reference_pair(engine: &Engine, bytes: &[u8]) -> (i32, i64) {
    let module =
        ReferenceModule::new(engine, bytes).expect("multi-value fixture must compile in Wasmtime");
    let mut store = Store::new(engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("multi-value fixture must instantiate in Wasmtime");
    let run = instance
        .get_typed_func::<(), (i32, i64)>(&mut store, "run")
        .expect("multi-value run export must be [] -> [i32, i64]");
    run.call(&mut store, ())
        .expect("multi-value Wasmtime invocation must not trap")
}

#[test]
fn generated_structured_multi_value_results_match_wasmtime() {
    const SEED: u64 = 0x510e_527f_ade6_82d1;
    let mut rng = XorShift64::new(SEED);
    let engine = Engine::default();

    for case in 0..96 {
        let condition = rng.next_i32();
        let then_i32 = rng.next_i32();
        let else_i32 = rng.next_i32();
        let then_i64 = rng.next_i64();
        let else_i64 = rng.next_i64();
        let wat = format!(
            "(module
                (func (export \"run\") (result i32 i64)
                    i32.const {condition}
                    if (result i32 i64)
                        i32.const {then_i32}
                        i64.const {then_i64}
                    else
                        i32.const {else_i32}
                        i64.const {else_i64}
                    end))"
        );
        let bytes = wat::parse_str(&wat).unwrap_or_else(|error| {
            panic!("generated multi-value WAT failed at seed={SEED:#018x} case={case}: {error}")
        });
        let expected = if condition != 0 {
            (then_i32, then_i64)
        } else {
            (else_i32, else_i64)
        };
        let mini = run_mini_pair(&bytes);
        let reference = run_reference_pair(&engine, &bytes);

        assert_eq!(
            mini, expected,
            "mini multi-value mismatch at seed={SEED:#018x} case={case}: condition={condition}"
        );
        assert_eq!(
            reference, expected,
            "Wasmtime multi-value mismatch at seed={SEED:#018x} case={case}: condition={condition}"
        );
        assert_eq!(
            mini, reference,
            "multi-value differential mismatch at seed={SEED:#018x} case={case}: condition={condition}"
        );
    }
}
