use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};

use wasm_parser::{parse_module, ValueType};
use wasm_runtime::{HostCapabilities, HostRegistry, Instance as MiniInstance, Value};
use wasmtime::{
    Engine, Extern, Func, Instance as ReferenceInstance, Module as ReferenceModule, Store,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct TraceOutcome {
    results: Vec<i64>,
    final_state: i64,
    failure_at: Option<usize>,
}

#[derive(Debug)]
struct ImportRegression {
    id: String,
    fixture: PathBuf,
    initial_state: i64,
    salt: i64,
    inputs: Vec<i64>,
    expected: TraceOutcome,
}

fn parse_i64(value: &str, field: &str, line_number: usize) -> i64 {
    value.parse().unwrap_or_else(|error| {
        panic!("manifest line {line_number} has invalid {field} {value:?}: {error}")
    })
}

fn parse_i64_list(value: &str, field: &str, line_number: usize) -> Vec<i64> {
    assert!(
        !value.is_empty(),
        "manifest line {line_number} has empty {field}"
    );
    value
        .split(',')
        .map(|item| parse_i64(item, field, line_number))
        .collect()
}

fn validate_relative_fixture_path(path: &Path) {
    assert!(
        !path.as_os_str().is_empty(),
        "import regression fixture path is empty"
    );
    assert!(
        !path.is_absolute(),
        "import regression fixture path must be relative"
    );
    for component in path.components() {
        assert!(
            matches!(component, Component::Normal(_)),
            "import regression fixture path must not escape its fixture directory: {path:?}"
        );
    }
    assert_eq!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("wat"),
        "import regression fixture must be a .wat file: {path:?}"
    );
}

fn independent_expected(initial_state: i64, salt: i64, inputs: &[i64]) -> TraceOutcome {
    let mut state = initial_state;
    let mut results = Vec::with_capacity(inputs.len());
    for input in inputs {
        state = state.wrapping_add(*input);
        results.push(state ^ salt);
    }
    TraceOutcome {
        results,
        final_state: state,
        failure_at: None,
    }
}

fn load_manifest() -> Vec<ImportRegression> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import_regressions");
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
            8,
            "manifest line {line_number} must contain exactly eight tab-separated fields"
        );
        let id = fields[0].trim();
        let fixture = PathBuf::from(fields[1].trim());
        let behavior = fields[2].trim();
        assert!(
            !id.is_empty(),
            "manifest line {line_number} has an empty id"
        );
        validate_relative_fixture_path(&fixture);
        assert_eq!(
            behavior, "stateful_i64_add",
            "manifest line {line_number} has unsupported host behavior {behavior:?}"
        );
        assert!(
            ids.insert(id.to_owned()),
            "duplicate import regression id {id:?} on line {line_number}"
        );
        assert!(
            fixtures.insert(fixture.clone()),
            "duplicate import regression fixture {fixture:?} on line {line_number}"
        );

        let initial_state = parse_i64(fields[3].trim(), "initial_state", line_number);
        let salt = parse_i64(fields[4].trim(), "salt", line_number);
        let inputs = parse_i64_list(fields[5].trim(), "inputs", line_number);
        let expected_results = parse_i64_list(fields[6].trim(), "expected_results", line_number);
        let expected_final_state = parse_i64(fields[7].trim(), "expected_final_state", line_number);
        assert_eq!(
            inputs.len(),
            expected_results.len(),
            "manifest line {line_number} must have one expected result per input"
        );

        let expected = TraceOutcome {
            results: expected_results,
            final_state: expected_final_state,
            failure_at: None,
        };
        assert_eq!(
            expected,
            independent_expected(initial_state, salt, &inputs),
            "manifest line {line_number} disagrees with the independent stateful_i64_add model"
        );

        let full_path = root.join(&fixture);
        assert!(
            full_path.is_file(),
            "import regression fixture does not exist: {full_path:?}"
        );
        regressions.push(ImportRegression {
            id: id.to_owned(),
            fixture: full_path,
            initial_state,
            salt,
            inputs,
            expected,
        });
    }

    assert!(
        !regressions.is_empty(),
        "import regression manifest must contain at least one fixture"
    );
    regressions
}

fn run_mini(bytes: &[u8], regression: &ImportRegression) -> TraceOutcome {
    let state = Arc::new(Mutex::new(regression.initial_state));
    let callback_state = Arc::clone(&state);
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "host",
            vec![ValueType::I64],
            vec![ValueType::I64],
            HostCapabilities::NONE,
            move |_ctx, args| {
                let mut value = callback_state
                    .lock()
                    .expect("mini import-regression host-state mutex poisoned");
                *value = value.wrapping_add(args[0].as_i64());
                Ok(Some(Value::I64(*value)))
            },
        )
        .expect("register mini import-regression host function");
    let module = parse_module(bytes).expect("import regression fixture must parse in mini runtime");
    let mut instance = MiniInstance::with_hosts(module, hosts)
        .expect("import regression fixture must validate and instantiate in mini runtime");

    let mut results = Vec::with_capacity(regression.inputs.len());
    let mut failure_at = None;
    for (call, input) in regression.inputs.iter().copied().enumerate() {
        match instance.invoke_export_values("run", &[Value::I64(input)]) {
            Ok(values) => match values.as_slice() {
                [Value::I64(value)] => results.push(*value),
                other => panic!("unexpected mini import-regression result shape: {other:?}"),
            },
            Err(_) => {
                failure_at = Some(call);
                break;
            }
        }
    }
    let final_state = *state
        .lock()
        .expect("read mini import-regression host state");
    TraceOutcome {
        results,
        final_state,
        failure_at,
    }
}

fn run_reference(engine: &Engine, bytes: &[u8], regression: &ImportRegression) -> TraceOutcome {
    let module =
        ReferenceModule::new(engine, bytes).expect("import regression fixture must compile");
    let state = Arc::new(Mutex::new(regression.initial_state));
    let callback_state = Arc::clone(&state);
    let mut store = Store::new(engine, ());
    let host = Func::wrap(&mut store, move |input: i64| -> i64 {
        let mut value = callback_state
            .lock()
            .expect("Wasmtime import-regression host-state mutex poisoned");
        *value = value.wrapping_add(input);
        *value
    });
    let instance = ReferenceInstance::new(&mut store, &module, &[Extern::Func(host)])
        .expect("import regression fixture must instantiate in Wasmtime");
    let run = instance
        .get_typed_func::<i64, i64>(&mut store, "run")
        .expect("import regression run export must be [i64] -> [i64]");

    let mut results = Vec::with_capacity(regression.inputs.len());
    let mut failure_at = None;
    for (call, input) in regression.inputs.iter().copied().enumerate() {
        match run.call(&mut store, input) {
            Ok(value) => results.push(value),
            Err(_) => {
                failure_at = Some(call);
                break;
            }
        }
    }
    let final_state = *state
        .lock()
        .expect("read Wasmtime import-regression host state");
    TraceOutcome {
        results,
        final_state,
        failure_at,
    }
}

#[test]
fn import_regression_fixtures_replay_against_wasmtime() {
    let regressions = load_manifest();
    let engine = Engine::default();

    for regression in regressions {
        let wat = fs::read_to_string(&regression.fixture).unwrap_or_else(|error| {
            panic!(
                "failed to read import regression fixture {:?} for {}: {error}",
                regression.fixture, regression.id
            )
        });
        let bytes = wat::parse_str(&wat).unwrap_or_else(|error| {
            panic!(
                "failed to compile import regression fixture {:?} for {}: {error}",
                regression.fixture, regression.id
            )
        });
        let mini = run_mini(&bytes, &regression);
        let reference = run_reference(&engine, &bytes, &regression);

        assert_eq!(
            mini, regression.expected,
            "mini runtime import regression replay failed for {} (salt={})",
            regression.id, regression.salt
        );
        assert_eq!(
            reference, regression.expected,
            "Wasmtime import regression expectation failed for {} (salt={})",
            regression.id, regression.salt
        );
        assert_eq!(
            mini, reference,
            "cross-engine import regression mismatch for {}",
            regression.id
        );
    }
}
