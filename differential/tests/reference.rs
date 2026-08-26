use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, RuntimeError, Value};
use wasmtime::{
    Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store, Trap as ReferenceTrap,
};

#[derive(Debug, Clone, Copy)]
enum ResultKind {
    I32,
    I64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrapClass {
    MemoryOutOfBounds,
    IntegerOverflow,
    IntegerDivisionByZero,
    BadConversionToInteger,
    Unreachable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    I32(i32),
    I64(i64),
    Trap(TrapClass),
}

struct Case {
    name: &'static str,
    wat: &'static str,
    kind: ResultKind,
    expected: Outcome,
}

fn normalize_mini_error(error: RuntimeError) -> TrapClass {
    match error {
        RuntimeError::MemoryOutOfBounds { .. } | RuntimeError::DataSegmentOutOfBounds { .. } => {
            TrapClass::MemoryOutOfBounds
        }
        RuntimeError::IntegerOverflow => TrapClass::IntegerOverflow,
        RuntimeError::IntegerDivisionByZero => TrapClass::IntegerDivisionByZero,
        RuntimeError::InvalidConversionToInteger => TrapClass::BadConversionToInteger,
        RuntimeError::Unreachable => TrapClass::Unreachable,
        other => panic!("unmapped mini-runtime differential error: {other:?}"),
    }
}

fn normalize_reference_error(error: &wasmtime::Error) -> TrapClass {
    let trap = error
        .downcast_ref::<ReferenceTrap>()
        .unwrap_or_else(|| panic!("Wasmtime execution error was not a trap: {error:?}"));
    match *trap {
        ReferenceTrap::MemoryOutOfBounds => TrapClass::MemoryOutOfBounds,
        ReferenceTrap::IntegerOverflow => TrapClass::IntegerOverflow,
        ReferenceTrap::IntegerDivisionByZero => TrapClass::IntegerDivisionByZero,
        ReferenceTrap::BadConversionToInteger => TrapClass::BadConversionToInteger,
        ReferenceTrap::UnreachableCodeReached => TrapClass::Unreachable,
        other => panic!("unmapped Wasmtime differential trap: {other:?}"),
    }
}

fn run_mini(bytes: &[u8], kind: ResultKind) -> Outcome {
    let module = parse_module(bytes).expect("differential fixture must parse in mini runtime");
    let mut instance =
        MiniInstance::new(module).expect("differential fixture must validate/instantiate");
    match instance.invoke_export_values("run", &[]) {
        Err(error) => Outcome::Trap(normalize_mini_error(error)),
        Ok(values) => match (kind, values.as_slice()) {
            (ResultKind::I32, [Value::I32(value)]) => Outcome::I32(*value),
            (ResultKind::I64, [Value::I64(value)]) => Outcome::I64(*value),
            _ => panic!("unexpected mini-runtime result shape: {values:?}"),
        },
    }
}

fn run_reference(engine: &Engine, bytes: &[u8], kind: ResultKind) -> Outcome {
    let module = ReferenceModule::new(engine, bytes).expect("fixture must compile in Wasmtime");
    let mut store = Store::new(engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("fixture must instantiate in Wasmtime");

    match kind {
        ResultKind::I32 => {
            let run = instance
                .get_typed_func::<(), i32>(&mut store, "run")
                .expect("run export must have [] -> [i32] signature");
            match run.call(&mut store, ()) {
                Ok(value) => Outcome::I32(value),
                Err(error) => Outcome::Trap(normalize_reference_error(&error)),
            }
        }
        ResultKind::I64 => {
            let run = instance
                .get_typed_func::<(), i64>(&mut store, "run")
                .expect("run export must have [] -> [i64] signature");
            match run.call(&mut store, ()) {
                Ok(value) => Outcome::I64(value),
                Err(error) => Outcome::Trap(normalize_reference_error(&error)),
            }
        }
    }
}

#[test]
fn supported_semantics_match_wasmtime_reference() {
    let cases = [
        Case {
            name: "i32 wrapping add",
            wat: r#"(module
                (func (export "run") (result i32)
                    i32.const 2147483647
                    i32.const 1
                    i32.add))"#,
            kind: ResultKind::I32,
            expected: Outcome::I32(i32::MIN),
        },
        Case {
            name: "i64 rotate",
            wat: r#"(module
                (func (export "run") (result i64)
                    i64.const 81985529216486895
                    i64.const 13
                    i64.rotl))"#,
            kind: ResultKind::I64,
            expected: Outcome::I64(0x68acf13579bde024_u64 as i64),
        },
        Case {
            name: "nop drop and select",
            wat: r#"(module
                (func (export "run") (result i32)
                    i32.const 99
                    drop
                    nop
                    i32.const 10
                    i32.const 20
                    i32.const 0
                    select))"#,
            kind: ResultKind::I32,
            expected: Outcome::I32(20),
        },
        Case {
            name: "br_table indexed target",
            wat: r#"(module
                (func (export "run") (result i32)
                    block (result i32)
                        block (result i32)
                            i32.const 40
                            i32.const 0
                            br_table 0 1
                        end
                        i32.const 2
                        i32.add
                    end))"#,
            kind: ResultKind::I32,
            expected: Outcome::I32(42),
        },
        Case {
            name: "br_table default target",
            wat: r#"(module
                (func (export "run") (result i32)
                    block (result i32)
                        block (result i32)
                            i32.const 40
                            i32.const 7
                            br_table 0 1
                        end
                        i32.const 2
                        i32.add
                    end))"#,
            kind: ResultKind::I32,
            expected: Outcome::I32(40),
        },
        Case {
            name: "f32 copysign bit pattern",
            wat: r#"(module
                (func (export "run") (result i32)
                    f32.const 1.5
                    f32.const -0.0
                    f32.copysign
                    i32.reinterpret_f32))"#,
            kind: ResultKind::I32,
            expected: Outcome::I32(0xbfc00000_u32 as i32),
        },
        Case {
            name: "f64 min signed zero",
            wat: r#"(module
                (func (export "run") (result i64)
                    f64.const 0.0
                    f64.const -0.0
                    f64.min
                    i64.reinterpret_f64))"#,
            kind: ResultKind::I64,
            expected: Outcome::I64(i64::MIN),
        },
        Case {
            name: "typed memory store load",
            wat: r#"(module
                (memory 1)
                (func (export "run") (result i64)
                    i32.const 16
                    i64.const 1234605616436508552
                    i64.store
                    i32.const 16
                    i64.load))"#,
            kind: ResultKind::I64,
            expected: Outcome::I64(0x1122334455667788),
        },
        Case {
            name: "memory grow returns previous size",
            wat: r#"(module
                (memory 1 2)
                (func (export "run") (result i32)
                    i32.const 1
                    memory.grow))"#,
            kind: ResultKind::I32,
            expected: Outcome::I32(1),
        },
        Case {
            name: "start function mutates memory before invocation",
            wat: r#"(module
                (memory 1)
                (data (i32.const 0) "A")
                (func $inc
                    i32.const 0
                    i32.const 0
                    i32.load8_u
                    i32.const 1
                    i32.add
                    i32.store8)
                (func $start
                    call $inc
                    call $inc
                    call $inc)
                (start $start)
                (func (export "run") (result i32)
                    i32.const 0
                    i32.load8_u))"#,
            kind: ResultKind::I32,
            expected: Outcome::I32(68),
        },
        Case {
            name: "unreachable traps",
            wat: r#"(module
                (func (export "run") (result i32)
                    unreachable))"#,
            kind: ResultKind::I32,
            expected: Outcome::Trap(TrapClass::Unreachable),
        },
        Case {
            name: "integer divide by zero traps",
            wat: r#"(module
                (func (export "run") (result i32)
                    i32.const 7
                    i32.const 0
                    i32.div_s))"#,
            kind: ResultKind::I32,
            expected: Outcome::Trap(TrapClass::IntegerDivisionByZero),
        },
        Case {
            name: "signed integer division overflow traps",
            wat: r#"(module
                (func (export "run") (result i32)
                    i32.const -2147483648
                    i32.const -1
                    i32.div_s))"#,
            kind: ResultKind::I32,
            expected: Outcome::Trap(TrapClass::IntegerOverflow),
        },
        Case {
            name: "memory out of bounds traps",
            wat: r#"(module
                (memory 1)
                (func (export "run") (result i32)
                    i32.const 65536
                    i32.load))"#,
            kind: ResultKind::I32,
            expected: Outcome::Trap(TrapClass::MemoryOutOfBounds),
        },
        Case {
            name: "invalid float conversion traps",
            wat: r#"(module
                (func (export "run") (result i32)
                    f32.const nan
                    i32.trunc_f32_s))"#,
            kind: ResultKind::I32,
            expected: Outcome::Trap(TrapClass::BadConversionToInteger),
        },
    ];

    let engine = Engine::default();
    for case in cases {
        let bytes = wat::parse_str(case.wat)
            .unwrap_or_else(|error| panic!("failed to compile WAT for {}: {error}", case.name));
        let mini = run_mini(&bytes, case.kind);
        let reference = run_reference(&engine, &bytes, case.kind);
        assert_eq!(
            mini, case.expected,
            "mini runtime diverged for {}",
            case.name
        );
        assert_eq!(
            reference, case.expected,
            "Wasmtime reference produced unexpected result for {}",
            case.name
        );
        assert_eq!(mini, reference, "differential mismatch for {}", case.name);
    }
}

#[test]
fn active_data_oob_matches_wasmtime_reference_at_instantiation() {
    let wat = r#"(module
        (memory 0)
        (data (i32.const 0) "a"))"#;
    let bytes = wat::parse_str(wat).expect("active-data OOB WAT must compile");

    let parsed = parse_module(&bytes).expect("active-data OOB fixture must parse in mini runtime");
    let mini_error = match MiniInstance::new(parsed) {
        Ok(_) => panic!("mini runtime unexpectedly instantiated OOB active-data module"),
        Err(error) => error,
    };
    assert_eq!(
        normalize_mini_error(mini_error),
        TrapClass::MemoryOutOfBounds
    );

    let engine = Engine::default();
    let module =
        ReferenceModule::new(&engine, &bytes).expect("active-data OOB fixture must compile");
    let mut store = Store::new(&engine, ());
    let reference_error = match ReferenceInstance::new(&mut store, &module, &[]) {
        Ok(_) => panic!("Wasmtime unexpectedly instantiated OOB active-data module"),
        Err(error) => error,
    };
    assert_eq!(
        normalize_reference_error(&reference_error),
        TrapClass::MemoryOutOfBounds
    );
}

#[test]
fn trapping_start_matches_wasmtime_reference_at_instantiation() {
    let wat = r#"(module
        (func $start
            unreachable)
        (start $start))"#;
    let bytes = wat::parse_str(wat).expect("start-trap WAT must compile");

    let parsed = parse_module(&bytes).expect("start-trap fixture must parse in mini runtime");
    let mini_error = match MiniInstance::new(parsed) {
        Ok(_) => panic!("mini runtime unexpectedly instantiated trapping start module"),
        Err(error) => error,
    };
    assert_eq!(normalize_mini_error(mini_error), TrapClass::Unreachable);

    let engine = Engine::default();
    let module = ReferenceModule::new(&engine, &bytes).expect("start-trap fixture must compile");
    let mut store = Store::new(&engine, ());
    let reference_error = match ReferenceInstance::new(&mut store, &module, &[]) {
        Ok(_) => panic!("Wasmtime unexpectedly instantiated trapping start module"),
        Err(error) => error,
    };
    assert_eq!(
        normalize_reference_error(&reference_error),
        TrapClass::Unreachable
    );
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

#[test]
fn generated_i32_modules_match_wasmtime_reference() {
    const SEED: u64 = 0xbb67_ae85_84ca_a73b;
    let mut rng = XorShift64::new(SEED);
    let engine = Engine::default();

    for case in 0..96 {
        let lhs = rng.next_i32();
        let rhs = rng.next_i32();
        let (operator, expected) = match rng.next_u64() % 6 {
            0 => ("i32.add", lhs.wrapping_add(rhs)),
            1 => ("i32.sub", lhs.wrapping_sub(rhs)),
            2 => ("i32.mul", lhs.wrapping_mul(rhs)),
            3 => ("i32.and", lhs & rhs),
            4 => ("i32.or", lhs | rhs),
            _ => ("i32.xor", lhs ^ rhs),
        };
        let wat = format!(
            "(module (func (export \"run\") (result i32) \
             i32.const {lhs} i32.const {rhs} {operator}))"
        );
        let bytes = wat::parse_str(&wat).unwrap_or_else(|error| {
            panic!("generated WAT failed at seed={SEED:#018x} case={case}: {error}")
        });
        let mini = run_mini(&bytes, ResultKind::I32);
        let reference = run_reference(&engine, &bytes, ResultKind::I32);
        let expected = Outcome::I32(expected);

        assert_eq!(
            mini, expected,
            "mini generated mismatch at seed={SEED:#018x} case={case}: \
             lhs={lhs}, rhs={rhs}, operator={operator}"
        );
        assert_eq!(
            reference, expected,
            "Wasmtime generated mismatch at seed={SEED:#018x} case={case}: \
             lhs={lhs}, rhs={rhs}, operator={operator}"
        );
        assert_eq!(
            mini, reference,
            "generated differential mismatch at seed={SEED:#018x} case={case}: \
             lhs={lhs}, rhs={rhs}, operator={operator}"
        );
    }
}
