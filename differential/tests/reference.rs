use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

#[derive(Debug, Clone, Copy)]
enum ResultKind {
    I32,
    I64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    I32(i32),
    I64(i64),
    Trap,
}

struct Case {
    name: &'static str,
    wat: &'static str,
    kind: ResultKind,
    expected: Outcome,
}

fn run_mini(bytes: &[u8], kind: ResultKind) -> Outcome {
    let module = parse_module(bytes).expect("differential fixture must parse in mini runtime");
    let mut instance =
        MiniInstance::new(module).expect("differential fixture must validate/instantiate");
    match instance.invoke_export_values("run", &[]) {
        Err(_) => Outcome::Trap,
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
            run.call(&mut store, ())
                .map(Outcome::I32)
                .unwrap_or(Outcome::Trap)
        }
        ResultKind::I64 => {
            let run = instance
                .get_typed_func::<(), i64>(&mut store, "run")
                .expect("run export must have [] -> [i64] signature");
            run.call(&mut store, ())
                .map(Outcome::I64)
                .unwrap_or(Outcome::Trap)
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
            name: "integer divide by zero traps",
            wat: r#"(module
                (func (export "run") (result i32)
                    i32.const 7
                    i32.const 0
                    i32.div_s))"#,
            kind: ResultKind::I32,
            expected: Outcome::Trap,
        },
        Case {
            name: "memory out of bounds traps",
            wat: r#"(module
                (memory 1)
                (func (export "run") (result i32)
                    i32.const 65536
                    i32.load))"#,
            kind: ResultKind::I32,
            expected: Outcome::Trap,
        },
        Case {
            name: "invalid float conversion traps",
            wat: r#"(module
                (func (export "run") (result i32)
                    f32.const nan
                    i32.trunc_f32_s))"#,
            kind: ResultKind::I32,
            expected: Outcome::Trap,
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
