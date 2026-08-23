use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
};

use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, RuntimeError, Value};
use wasmtime::{
    Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store, Trap as ReferenceTrap,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrapClass {
    MemoryOutOfBounds,
    TableOutOfBounds,
    IndirectCallToNull,
    BadSignature,
    IntegerOverflow,
    IntegerDivisionByZero,
    BadConversionToInteger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    I32(i32),
    I64(i64),
    PairI32I64(i32, i64),
    Trap(TrapClass),
}

#[derive(Debug)]
struct Regression {
    id: String,
    fixture: PathBuf,
    expected: Outcome,
}

fn parse_trap_class(value: &str) -> TrapClass {
    match value {
        "memory_out_of_bounds" => TrapClass::MemoryOutOfBounds,
        "table_out_of_bounds" => TrapClass::TableOutOfBounds,
        "indirect_call_to_null" => TrapClass::IndirectCallToNull,
        "bad_signature" => TrapClass::BadSignature,
        "integer_overflow" => TrapClass::IntegerOverflow,
        "integer_division_by_zero" => TrapClass::IntegerDivisionByZero,
        "bad_conversion_to_integer" => TrapClass::BadConversionToInteger,
        other => panic!("unknown regression trap class {other:?}"),
    }
}

fn parse_expected(kind: &str, value: &str) -> Outcome {
    match kind {
        "i32" => Outcome::I32(
            value
                .parse()
                .unwrap_or_else(|error| panic!("invalid i32 expectation {value:?}: {error}")),
        ),
        "i64" => Outcome::I64(
            value
                .parse()
                .unwrap_or_else(|error| panic!("invalid i64 expectation {value:?}: {error}")),
        ),
        "pair_i32_i64" => {
            let mut parts = value.split(',');
            let first = parts
                .next()
                .expect("pair expectation must contain i32 component")
                .parse()
                .unwrap_or_else(|error| panic!("invalid pair i32 component {value:?}: {error}"));
            let second = parts
                .next()
                .expect("pair expectation must contain i64 component")
                .parse()
                .unwrap_or_else(|error| panic!("invalid pair i64 component {value:?}: {error}"));
            assert!(
                parts.next().is_none(),
                "pair expectation must contain exactly two comma-separated values: {value:?}"
            );
            Outcome::PairI32I64(first, second)
        }
        "trap" => Outcome::Trap(parse_trap_class(value)),
        other => panic!("unknown regression outcome kind {other:?}"),
    }
}

fn validate_relative_fixture_path(path: &Path) {
    assert!(!path.as_os_str().is_empty(), "regression fixture path is empty");
    assert!(!path.is_absolute(), "regression fixture path must be relative");
    for component in path.components() {
        assert!(
            matches!(component, Component::Normal(_)),
            "regression fixture path must not escape its fixture directory: {path:?}"
        );
    }
    assert_eq!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("wat"),
        "regression fixture must be a .wat file: {path:?}"
    );
}

fn load_manifest() -> Vec<Regression> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/regressions");
    let manifest_path = root.join("manifest.tsv");
    let manifest = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|error| panic!("failed to read {manifest_path:?}: {error}"));
    let mut ids = HashSet::new();
    let mut fixtures = HashSet::new();
    let mut regressions = Vec::new();

    for (line_index, raw_line) in manifest.lines().enumerate() {
        let line_number = line_index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let fields: Vec<&str> = raw_line.split('\t').collect();
        assert_eq!(
            fields.len(),
            4,
            "manifest line {line_number} must contain exactly four tab-separated fields"
        );
        let id = fields[0].trim();
        let fixture = PathBuf::from(fields[1].trim());
        let kind = fields[2].trim();
        let expected = fields[3].trim();
        assert!(!id.is_empty(), "manifest line {line_number} has an empty id");
        validate_relative_fixture_path(&fixture);
        assert!(
            ids.insert(id.to_owned()),
            "duplicate regression id {id:?} on line {line_number}"
        );
        assert!(
            fixtures.insert(fixture.clone()),
            "duplicate regression fixture {fixture:?} on line {line_number}"
        );
        let full_path = root.join(&fixture);
        assert!(
            full_path.is_file(),
            "regression fixture does not exist: {full_path:?}"
        );
        regressions.push(Regression {
            id: id.to_owned(),
            fixture: full_path,
            expected: parse_expected(kind, expected),
        });
    }

    assert!(
        !regressions.is_empty(),
        "regression manifest must contain at least one fixture"
    );
    regressions
}

fn normalize_mini_error(error: RuntimeError) -> TrapClass {
    match error {
        RuntimeError::MemoryOutOfBounds { .. } => TrapClass::MemoryOutOfBounds,
        RuntimeError::TableElementOutOfBounds(_) => TrapClass::TableOutOfBounds,
        RuntimeError::UninitializedTableElement(_) => TrapClass::IndirectCallToNull,
        RuntimeError::IndirectCallTypeMismatch { .. } => TrapClass::BadSignature,
        RuntimeError::IntegerOverflow => TrapClass::IntegerOverflow,
        RuntimeError::IntegerDivisionByZero => TrapClass::IntegerDivisionByZero,
        RuntimeError::InvalidConversionToInteger => TrapClass::BadConversionToInteger,
        other => panic!("unmapped mini-runtime regression error: {other:?}"),
    }
}

fn normalize_reference_error(error: &wasmtime::Error) -> TrapClass {
    let trap = error
        .downcast_ref::<ReferenceTrap>()
        .unwrap_or_else(|| panic!("Wasmtime regression error was not a trap: {error:?}"));
    match *trap {
        ReferenceTrap::MemoryOutOfBounds => TrapClass::MemoryOutOfBounds,
        ReferenceTrap::TableOutOfBounds => TrapClass::TableOutOfBounds,
        ReferenceTrap::IndirectCallToNull => TrapClass::IndirectCallToNull,
        ReferenceTrap::BadSignature => TrapClass::BadSignature,
        ReferenceTrap::IntegerOverflow => TrapClass::IntegerOverflow,
        ReferenceTrap::IntegerDivisionByZero => TrapClass::IntegerDivisionByZero,
        ReferenceTrap::BadConversionToInteger => TrapClass::BadConversionToInteger,
        other => panic!("unmapped Wasmtime regression trap: {other:?}"),
    }
}

fn run_mini(bytes: &[u8], expected: Outcome) -> Outcome {
    let module = parse_module(bytes).expect("regression fixture must parse in mini runtime");
    let mut instance =
        MiniInstance::new(module).expect("regression fixture must validate and instantiate");
    match instance.invoke_export_values("run", &[]) {
        Err(error) => Outcome::Trap(normalize_mini_error(error)),
        Ok(values) => match (expected, values.as_slice()) {
            (Outcome::I32(_), [Value::I32(value)]) => Outcome::I32(*value),
            (Outcome::I64(_), [Value::I64(value)]) => Outcome::I64(*value),
            (Outcome::PairI32I64(_, _), [Value::I32(first), Value::I64(second)]) => {
                Outcome::PairI32I64(*first, *second)
            }
            (Outcome::Trap(_), values) => {
                panic!("regression expected a trap but mini runtime returned {values:?}")
            }
            (_, values) => panic!("unexpected mini-runtime regression result shape: {values:?}"),
        },
    }
}

fn run_reference(engine: &Engine, bytes: &[u8], expected: Outcome) -> Outcome {
    let module = ReferenceModule::new(engine, bytes).expect("regression fixture must compile");
    let mut store = Store::new(engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("regression fixture must instantiate in Wasmtime");

    match expected {
        Outcome::I32(_) => {
            let run = instance
                .get_typed_func::<(), i32>(&mut store, "run")
                .expect("regression run export must be [] -> [i32]");
            match run.call(&mut store, ()) {
                Ok(value) => Outcome::I32(value),
                Err(error) => Outcome::Trap(normalize_reference_error(&error)),
            }
        }
        Outcome::I64(_) => {
            let run = instance
                .get_typed_func::<(), i64>(&mut store, "run")
                .expect("regression run export must be [] -> [i64]");
            match run.call(&mut store, ()) {
                Ok(value) => Outcome::I64(value),
                Err(error) => Outcome::Trap(normalize_reference_error(&error)),
            }
        }
        Outcome::PairI32I64(_, _) => {
            let run = instance
                .get_typed_func::<(), (i32, i64)>(&mut store, "run")
                .expect("regression run export must be [] -> [i32, i64]");
            match run.call(&mut store, ()) {
                Ok((first, second)) => Outcome::PairI32I64(first, second),
                Err(error) => Outcome::Trap(normalize_reference_error(&error)),
            }
        }
        Outcome::Trap(_) => {
            let run = instance
                .get_typed_func::<(), i32>(&mut store, "run")
                .expect("trapping regression run export must be [] -> [i32]");
            match run.call(&mut store, ()) {
                Ok(value) => Outcome::I32(value),
                Err(error) => Outcome::Trap(normalize_reference_error(&error)),
            }
        }
    }
}

#[test]
fn minimized_regression_fixtures_replay_against_wasmtime() {
    let regressions = load_manifest();
    let engine = Engine::default();

    for regression in regressions {
        let wat = fs::read_to_string(&regression.fixture).unwrap_or_else(|error| {
            panic!(
                "failed to read regression fixture {:?} for {}: {error}",
                regression.fixture, regression.id
            )
        });
        let bytes = wat::parse_str(&wat).unwrap_or_else(|error| {
            panic!(
                "failed to compile regression fixture {:?} for {}: {error}",
                regression.fixture, regression.id
            )
        });
        let mini = run_mini(&bytes, regression.expected);
        let reference = run_reference(&engine, &bytes, regression.expected);

        assert_eq!(
            mini, regression.expected,
            "mini runtime regression replay failed for {}",
            regression.id
        );
        assert_eq!(
            reference, regression.expected,
            "Wasmtime regression expectation failed for {}",
            regression.id
        );
        assert_eq!(
            mini, reference,
            "cross-engine regression mismatch for {}",
            regression.id
        );
    }
}
