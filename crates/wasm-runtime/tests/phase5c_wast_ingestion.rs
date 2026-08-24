use std::collections::HashSet;
use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};
use wast::core::{NanPattern, WastArgCore, WastRetCore};
use wast::parser::{self, ParseBuffer};
use wast::{QuoteWat, Wast, WastArg, WastDirective, WastExecute, WastInvoke, WastRet, Wat};

const CONTRACT_FIXTURE: &str = include_str!("fixtures/phase5c_ingestion_contract.wast");
const UPSTREAM_MANIFEST: &str = include_str!("fixtures/phase5c_upstream_manifest.tsv");
const UPSTREAM_ADDRESS_SUBSET: &str = include_str!("fixtures/phase5c_upstream_address_subset.wast");
const UPSTREAM_ALIGN_SUBSET: &str = include_str!("fixtures/phase5c_upstream_align_subset.wast");
const UPSTREAM_BLOCK_SUBSET: &str = include_str!("fixtures/phase5c_upstream_block_subset.wast");
const UPSTREAM_BR_SUBSET: &str = include_str!("fixtures/phase5c_upstream_br_subset.wast");
const UPSTREAM_BR_IF_SUBSET: &str = include_str!("fixtures/phase5c_upstream_br_if_subset.wast");
const UPSTREAM_CONVERSIONS_SUBSET: &str =
    include_str!("fixtures/phase5c_upstream_conversions_subset.wast");
const UPSTREAM_F32_CMP_SUBSET: &str = include_str!("fixtures/phase5c_upstream_f32_cmp_subset.wast");
const UPSTREAM_F32_SUBSET: &str = include_str!("fixtures/phase5c_upstream_f32_subset.wast");
const UPSTREAM_F64_CMP_SUBSET: &str = include_str!("fixtures/phase5c_upstream_f64_cmp_subset.wast");
const UPSTREAM_F64_SUBSET: &str = include_str!("fixtures/phase5c_upstream_f64_subset.wast");
const UPSTREAM_FLOAT_MEMORY_SUBSET: &str =
    include_str!("fixtures/phase5c_upstream_float_memory_subset.wast");
const UPSTREAM_FUNC_SUBSET: &str = include_str!("fixtures/phase5c_upstream_func_subset.wast");
const UPSTREAM_I32_SUBSET: &str = include_str!("fixtures/phase5c_upstream_i32_subset.wast");
const UPSTREAM_I64_SUBSET: &str = include_str!("fixtures/phase5c_upstream_i64_subset.wast");
const UPSTREAM_IF_SUBSET: &str = include_str!("fixtures/phase5c_upstream_if_subset.wast");
const UPSTREAM_LOCAL_GET_SUBSET: &str =
    include_str!("fixtures/phase5c_upstream_local_get_subset.wast");
const UPSTREAM_LOCAL_SET_SUBSET: &str =
    include_str!("fixtures/phase5c_upstream_local_set_subset.wast");
const UPSTREAM_LOCAL_TEE_SUBSET: &str =
    include_str!("fixtures/phase5c_upstream_local_tee_subset.wast");
const UPSTREAM_LOOP_SUBSET: &str = include_str!("fixtures/phase5c_upstream_loop_subset.wast");
const UPSTREAM_MEMORY_GROW_SUBSET: &str =
    include_str!("fixtures/phase5c_upstream_memory_grow_subset.wast");
const UPSTREAM_MEMORY_SUBSET: &str = include_str!("fixtures/phase5c_upstream_memory_subset.wast");
const UPSTREAM_MEMORY_TRAP_SUBSET: &str =
    include_str!("fixtures/phase5c_upstream_memory_trap_subset.wast");
const UPSTREAM_RETURN_SUBSET: &str = include_str!("fixtures/phase5c_upstream_return_subset.wast");
const PINNED_UPSTREAM_SPEC_COMMIT: &str = "fc209c5ed8afc4dfeb9252024d217da3376c7a6f";

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
    executed_invocations: usize,
    skipped: Vec<FilterReason>,
}

#[derive(Debug)]
struct ManifestEntry<'a> {
    source: &'a str,
    fixture: &'a str,
    expected_modules: usize,
    expected_executed_assertions: usize,
    expected_executed_invocations: usize,
    expected_filtered: usize,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

fn translate_wast_invoke(invoke: WastInvoke<'_>) -> Result<(&str, Vec<Value>), FilterReason> {
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

fn translate_invoke(exec: WastExecute<'_>) -> Result<(&str, Vec<Value>), FilterReason> {
    match exec {
        WastExecute::Invoke(invoke) => translate_wast_invoke(invoke),
        other => Err(FilterReason::UnsupportedExecution(format!("{other:?}"))),
    }
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
                instance =
                    Some(Instance::new(parsed).expect("encoded supported module must instantiate"));
                report.modules += 1;
            }
            WastDirective::Invoke(invoke) => {
                let (name, args) = match translate_wast_invoke(invoke) {
                    Ok(invoke) => invoke,
                    Err(reason) => {
                        report.skipped.push(reason);
                        continue;
                    }
                };
                let actual = instance
                    .as_mut()
                    .expect("bare invoke requires a preceding supported module")
                    .invoke_export_values(name, &args)
                    .expect("bare invoke must not trap");
                assert!(
                    actual.is_empty(),
                    "bare invoke support is intentionally limited to zero-result exports: {name} returned {actual:?}"
                );
                report.executed_invocations += 1;
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
                for (index, (expected, actual)) in expected.iter().zip(actual).enumerate() {
                    assert!(
                        value_matches(expected, actual),
                        "assert_return mismatch for export {name} result {index}: expected {expected:?}, actual {actual:?}"
                    );
                }
                report.executed_assertions += 1;
            }
            WastDirective::AssertTrap { exec, message, .. } => {
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

fn parse_manifest(source: &str) -> Vec<ManifestEntry<'_>> {
    source
        .lines()
        .enumerate()
        .filter_map(|(line_index, raw_line)| {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }

            let fields = line.split('\t').collect::<Vec<_>>();
            assert_eq!(
                fields.len(),
                7,
                "manifest line {} must contain exactly seven tab-separated fields",
                line_index + 1
            );
            assert_eq!(
                fields[0],
                PINNED_UPSTREAM_SPEC_COMMIT,
                "manifest line {} drifted from the pinned upstream commit",
                line_index + 1
            );
            assert!(
                fields[1].starts_with("test/core/") && fields[1].ends_with(".wast"),
                "manifest line {} must name an upstream core .wast source",
                line_index + 1
            );

            let parse_count = |field: &str, label: &str| {
                field.parse::<usize>().unwrap_or_else(|error| {
                    panic!(
                        "manifest line {} has invalid {label} count {field:?}: {error}",
                        line_index + 1
                    )
                })
            };

            Some(ManifestEntry {
                source: fields[1],
                fixture: fields[2],
                expected_modules: parse_count(fields[3], "module"),
                expected_executed_assertions: parse_count(fields[4], "executed assertion"),
                expected_executed_invocations: parse_count(fields[5], "executed invocation"),
                expected_filtered: parse_count(fields[6], "filtered directive"),
            })
        })
        .collect()
}

fn manifest_fixture(name: &str) -> &'static str {
    match name {
        "phase5c_upstream_address_subset.wast" => UPSTREAM_ADDRESS_SUBSET,
        "phase5c_upstream_align_subset.wast" => UPSTREAM_ALIGN_SUBSET,
        "phase5c_upstream_block_subset.wast" => UPSTREAM_BLOCK_SUBSET,
        "phase5c_upstream_br_subset.wast" => UPSTREAM_BR_SUBSET,
        "phase5c_upstream_br_if_subset.wast" => UPSTREAM_BR_IF_SUBSET,
        "phase5c_upstream_conversions_subset.wast" => UPSTREAM_CONVERSIONS_SUBSET,
        "phase5c_upstream_f32_cmp_subset.wast" => UPSTREAM_F32_CMP_SUBSET,
        "phase5c_upstream_f32_subset.wast" => UPSTREAM_F32_SUBSET,
        "phase5c_upstream_f64_cmp_subset.wast" => UPSTREAM_F64_CMP_SUBSET,
        "phase5c_upstream_f64_subset.wast" => UPSTREAM_F64_SUBSET,
        "phase5c_upstream_float_memory_subset.wast" => UPSTREAM_FLOAT_MEMORY_SUBSET,
        "phase5c_upstream_func_subset.wast" => UPSTREAM_FUNC_SUBSET,
        "phase5c_upstream_i32_subset.wast" => UPSTREAM_I32_SUBSET,
        "phase5c_upstream_i64_subset.wast" => UPSTREAM_I64_SUBSET,
        "phase5c_upstream_if_subset.wast" => UPSTREAM_IF_SUBSET,
        "phase5c_upstream_local_get_subset.wast" => UPSTREAM_LOCAL_GET_SUBSET,
        "phase5c_upstream_local_set_subset.wast" => UPSTREAM_LOCAL_SET_SUBSET,
        "phase5c_upstream_local_tee_subset.wast" => UPSTREAM_LOCAL_TEE_SUBSET,
        "phase5c_upstream_loop_subset.wast" => UPSTREAM_LOOP_SUBSET,
        "phase5c_upstream_memory_grow_subset.wast" => UPSTREAM_MEMORY_GROW_SUBSET,
        "phase5c_upstream_memory_subset.wast" => UPSTREAM_MEMORY_SUBSET,
        "phase5c_upstream_memory_trap_subset.wast" => UPSTREAM_MEMORY_TRAP_SUBSET,
        "phase5c_upstream_return_subset.wast" => UPSTREAM_RETURN_SUBSET,
        other => panic!("manifest names unregistered fixture {other:?}"),
    }
}

#[test]
fn systematic_wast_ingestion_executes_supported_subset_and_reports_filters() {
    let report = run_fixture(CONTRACT_FIXTURE);

    assert_eq!(report.modules, 1);
    assert_eq!(report.executed_assertions, 5);
    assert_eq!(report.executed_invocations, 1);
    assert_eq!(report.skipped.len(), 2);
    assert!(matches!(
        &report.skipped[0],
        FilterReason::UnsupportedExecution(detail) if detail.starts_with("Get")
    ));
    assert!(matches!(
        &report.skipped[1],
        FilterReason::UnsupportedDirective(detail) if detail.starts_with("Register")
    ));
}

#[test]
fn pinned_upstream_manifest_executes_with_exact_accounting() {
    let entries = parse_manifest(UPSTREAM_MANIFEST);
    assert!(
        !entries.is_empty(),
        "pinned upstream manifest must not be empty"
    );

    let mut seen_sources = HashSet::new();
    let mut seen_fixtures = HashSet::new();

    for entry in entries {
        assert!(
            seen_sources.insert(entry.source),
            "pinned upstream manifest repeats source {}",
            entry.source
        );
        assert!(
            seen_fixtures.insert(entry.fixture),
            "pinned upstream manifest repeats fixture {}",
            entry.fixture
        );

        let fixture = manifest_fixture(entry.fixture);
        assert!(
            fixture.contains(PINNED_UPSTREAM_SPEC_COMMIT),
            "fixture {} does not record pinned commit {}",
            entry.fixture,
            PINNED_UPSTREAM_SPEC_COMMIT
        );
        assert!(
            fixture.contains(entry.source),
            "fixture {} does not record upstream source {}",
            entry.fixture,
            entry.source
        );

        let report = run_fixture(fixture);
        assert_eq!(
            report.modules, entry.expected_modules,
            "upstream source {} module accounting drifted",
            entry.source
        );
        assert_eq!(
            report.executed_assertions, entry.expected_executed_assertions,
            "upstream source {} executed-assertion accounting drifted",
            entry.source
        );
        assert_eq!(
            report.executed_invocations, entry.expected_executed_invocations,
            "upstream source {} executed-invocation accounting drifted",
            entry.source
        );
        assert_eq!(
            report.skipped.len(),
            entry.expected_filtered,
            "upstream source {} filtered-directive accounting drifted: {:?}",
            entry.source,
            report.skipped
        );
    }
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
