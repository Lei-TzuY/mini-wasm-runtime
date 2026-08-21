use wast::core::{NanPattern, WastArgCore, WastRetCore};
use wast::parser::{self, ParseBuffer};
use wast::{QuoteWat, Wast, WastArg, WastDirective, WastExecute, WastRet, Wat};
use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};

const CONTRACT_FIXTURE: &str = include_str!("fixtures/phase5c_ingestion_contract.wast");

#[derive(Debug, PartialEq, Eq)]
enum FilterReason {
    UnsupportedModule(String),
    UnsupportedDirective(String),
    UnsupportedExecution(String),
    NamedModuleInvocation,
    UnsupportedArgument(String),
    UnsupportedExpectedValue(String),
    UnsupportedTrapMessage(String),
}

#[derive(Debug, Default)]
struct IngestionReport {
    modules: usize,
    executed_assertions: usize,
    skipped: Vec<FilterReason>,
}

#[derive(Debug)]
enum ExpectedValue {
    I32(i32),
    I64(i64),
    F32Bits(u32),
    F64Bits(u64),
    F32CanonicalNan,
    F32ArithmeticNan,
    F64CanonicalNan,
    F64ArithmeticNan,
}

#[derive(Debug, Clone, Copy)]
enum TrapKind {
    IntegerDivisionByZero,
    IntegerOverflow,
    InvalidConversionToInteger,
    MemoryOutOfBounds,
}

fn is_supported_core_module(module: &QuoteWat<'_>) -> bool {
    matches!(module, QuoteWat::Wat(Wat::Module(_)))
}

fn translate_argument(arg: WastArg<'_>) -> Result<Value, FilterReason> {
    match arg {
        WastArg::Core(WastArgCore::I32(value)) => Ok(Value::I32(value)),
        WastArg::Core(WastArgCore::I64(value)) => Ok(Value::I64(value)),
        WastArg::Core(WastArgCore::F32(value)) => Ok(Value::F32(f32::from_bits(value.bits))),
        WastArg::Core(WastArgCore::F64(value)) => Ok(Value::F64(f64::from_bits(value.bits))),
        other => Err(FilterReason::UnsupportedArgument(format!("{other:?}"))),
    }
}

fn translate_expected(result: WastRet<'_>) -> Result<ExpectedValue, FilterReason> {
    match result {
        WastRet::Core(WastRetCore::I32(value)) => Ok(ExpectedValue::I32(value)),
        WastRet::Core(WastRetCore::I64(value)) => Ok(ExpectedValue::I64(value)),
        WastRet::Core(WastRetCore::F32(NanPattern::Value(value))) => {
            Ok(ExpectedValue::F32Bits(value.bits))
        }
        WastRet::Core(WastRetCore::F64(NanPattern::Value(value))) => {
            Ok(ExpectedValue::F64Bits(value.bits))
        }
        WastRet::Core(WastRetCore::F32(NanPattern::CanonicalNan)) => {
            Ok(ExpectedValue::F32CanonicalNan)
        }
        WastRet::Core(WastRetCore::F32(NanPattern::ArithmeticNan)) => {
            Ok(ExpectedValue::F32ArithmeticNan)
        }
        WastRet::Core(WastRetCore::F64(NanPattern::CanonicalNan)) => {
            Ok(ExpectedValue::F64CanonicalNan)
        }
        WastRet::Core(WastRetCore::F64(NanPattern::ArithmeticNan)) => {
            Ok(ExpectedValue::F64ArithmeticNan)
        }
        other => Err(FilterReason::UnsupportedExpectedValue(format!("{other:?}"))),
    }
}

fn translate_invoke(exec: WastExecute<'_>) -> Result<(&str, Vec<Value>), FilterReason> {
    let invoke = match exec {
        WastExecute::Invoke(invoke) => invoke,
        other => {
            return Err(FilterReason::UnsupportedExecution(format!("{other:?}")));
        }
    };
    if invoke.module.is_some() {
        return Err(FilterReason::NamedModuleInvocation);
    }
    let args = invoke
        .args
        .into_iter()
        .map(translate_argument)
        .collect::<Result<Vec<_>, _>>()?;
    Ok((invoke.name, args))
}

fn translate_trap(message: &str) -> Result<TrapKind, FilterReason> {
    match message {
        "integer divide by zero" => Ok(TrapKind::IntegerDivisionByZero),
        "integer overflow" => Ok(TrapKind::IntegerOverflow),
        "invalid conversion to integer" => Ok(TrapKind::InvalidConversionToInteger),
        "out of bounds memory access" => Ok(TrapKind::MemoryOutOfBounds),
        other => Err(FilterReason::UnsupportedTrapMessage(other.to_string())),
    }
}

fn is_canonical_f32_nan(bits: u32) -> bool {
    bits & 0x7fff_ffff == 0x7fc0_0000
}

fn is_arithmetic_f32_nan(bits: u32) -> bool {
    bits & 0x7fc0_0000 == 0x7fc0_0000
}

fn is_canonical_f64_nan(bits: u64) -> bool {
    bits & 0x7fff_ffff_ffff_ffff == 0x7ff8_0000_0000_0000
}

fn is_arithmetic_f64_nan(bits: u64) -> bool {
    bits & 0x7ff8_0000_0000_0000 == 0x7ff8_0000_0000_0000
}

fn value_matches(expected: &ExpectedValue, actual: Value) -> bool {
    match (expected, actual) {
        (ExpectedValue::I32(expected), Value::I32(actual)) => *expected == actual,
        (ExpectedValue::I64(expected), Value::I64(actual)) => *expected == actual,
        (ExpectedValue::F32Bits(expected), Value::F32(actual)) => *expected == actual.to_bits(),
        (ExpectedValue::F64Bits(expected), Value::F64(actual)) => *expected == actual.to_bits(),
        (ExpectedValue::F32CanonicalNan, Value::F32(actual)) => {
            is_canonical_f32_nan(actual.to_bits())
        }
        (ExpectedValue::F32ArithmeticNan, Value::F32(actual)) => {
            is_arithmetic_f32_nan(actual.to_bits())
        }
        (ExpectedValue::F64CanonicalNan, Value::F64(actual)) => {
            is_canonical_f64_nan(actual.to_bits())
        }
        (ExpectedValue::F64ArithmeticNan, Value::F64(actual)) => {
            is_arithmetic_f64_nan(actual.to_bits())
        }
        _ => false,
    }
}

fn trap_matches(expected: TrapKind, actual: &RuntimeError) -> bool {
    match expected {
        TrapKind::IntegerDivisionByZero => matches!(actual, RuntimeError::IntegerDivisionByZero),
        TrapKind::IntegerOverflow => matches!(actual, RuntimeError::IntegerOverflow),
        TrapKind::InvalidConversionToInteger => {
            matches!(actual, RuntimeError::InvalidConversionToInteger)
        }
        TrapKind::MemoryOutOfBounds => matches!(actual, RuntimeError::MemoryOutOfBounds { .. }),
    }
}

fn run_fixture(source: &str) -> IngestionReport {
    let buffer = ParseBuffer::new(source).expect("contract WAST must lex");
    let wast = parser::parse::<Wast<'_>>(&buffer).expect("contract WAST must parse");
    let mut report = IngestionReport::default();
    let mut instance: Option<Instance> = None;

    for directive in wast.directives {
        match directive {
            WastDirective::Module(mut module) => {
                if !is_supported_core_module(&module) {
                    report
                        .skipped
                        .push(FilterReason::UnsupportedModule(format!("{module:?}")));
                    continue;
                }
                let bytes = module.encode().expect("supported WAT module must encode");
                let parsed = parse_module(&bytes).expect("encoded supported module must parse");
                instance = Some(Instance::new(parsed).expect("encoded supported module must instantiate"));
                report.modules += 1;
            }
            WastDirective::AssertReturn { exec, results, .. } => {
                let (name, args) = match translate_invoke(exec) {
                    Ok(invoke) => invoke,
                    Err(reason) => {
                        report.skipped.push(reason);
                        continue;
                    }
                };
                let expected = match results
                    .into_iter()
                    .map(translate_expected)
                    .collect::<Result<Vec<_>, _>>()
                {
                    Ok(expected) => expected,
                    Err(reason) => {
                        report.skipped.push(reason);
                        continue;
                    }
                };
                let actual = instance
                    .as_mut()
                    .expect("assert_return requires a preceding supported module")
                    .invoke_export_values(name, &args)
                    .expect("assert_return invocation must not trap");
                assert_eq!(
                    actual.len(),
                    expected.len(),
                    "assert_return result arity mismatch for export {name}"
                );
                for (index, (expected, actual)) in
                    expected.iter().zip(actual.into_iter()).enumerate()
                {
                    assert!(
                        value_matches(expected, actual),
                        "assert_return mismatch for export {name} result {index}: expected {expected:?}, actual {actual:?}"
                    );
                }
                report.executed_assertions += 1;
            }
            WastDirective::AssertTrap {
                exec, message, ..
            } => {
                let expected_trap = match translate_trap(message) {
                    Ok(trap) => trap,
                    Err(reason) => {
                        report.skipped.push(reason);
                        continue;
                    }
                };
                let (name, args) = match translate_invoke(exec) {
                    Ok(invoke) => invoke,
                    Err(reason) => {
                        report.skipped.push(reason);
                        continue;
                    }
                };
                let error = instance
                    .as_mut()
                    .expect("assert_trap requires a preceding supported module")
                    .invoke_export_values(name, &args)
                    .expect_err("assert_trap invocation unexpectedly succeeded");
                assert!(
                    trap_matches(expected_trap, &error),
                    "assert_trap mismatch for export {name}: message={message:?}, runtime={error:?}"
                );
                report.executed_assertions += 1;
            }
            other => report
                .skipped
                .push(FilterReason::UnsupportedDirective(format!("{other:?}"))),
        }
    }

    report
}

#[test]
fn systematic_wast_ingestion_executes_supported_subset_and_reports_filters() {
    let report = run_fixture(CONTRACT_FIXTURE);

    assert_eq!(report.modules, 1);
    assert_eq!(report.executed_assertions, 4);
    assert_eq!(report.skipped.len(), 2);
    assert!(matches!(
        &report.skipped[0],
        FilterReason::UnsupportedDirective(detail) if detail.starts_with("AssertReturn")
    ));
    assert!(matches!(
        &report.skipped[1],
        FilterReason::UnsupportedDirective(detail) if detail.starts_with("Register")
    ));
}

#[test]
fn unsupported_trap_messages_are_explicit_filter_results() {
    assert_eq!(
        translate_trap("future trap wording"),
        Err(FilterReason::UnsupportedTrapMessage(
            "future trap wording".to_string()
        ))
    );
}
