use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
};

use wasm_parser::parse_module;
use wasm_runtime::{GlobalHandle, HostRegistry, Instance as MiniInstance, Value};
use wasmtime::{
    Engine, Extern, Global, GlobalType, Instance as ReferenceInstance, Module as ReferenceModule,
    Mutability, Store, Val, ValType,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct TraceOutcome {
    results: Vec<i32>,
    final_state: i32,
    failure_at: Option<usize>,
}

#[derive(Debug)]
struct GlobalRegression {
    id: String,
    fixture: PathBuf,
    initial_state: i32,
    override_call: usize,
    override_value: i32,
    inputs: Vec<i32>,
    expected: TraceOutcome,
}

fn parse_i32(value: &str, field: &str, line_number: usize) -> i32 {
    value.parse().unwrap_or_else(|error| {
        panic!("manifest line {line_number} has invalid {field} {value:?}: {error}")
    })
}

fn parse_usize(value: &str, field: &str, line_number: usize) -> usize {
    value.parse().unwrap_or_else(|error| {
        panic!("manifest line {line_number} has invalid {field} {value:?}: {error}")
    })
}

fn parse_i32_list(value: &str, field: &str, line_number: usize) -> Vec<i32> {
    assert!(
        !value.is_empty(),
        "manifest line {line_number} has empty {field}"
    );
    value
        .split(',')
        .map(|item| parse_i32(item, field, line_number))
        .collect()
}

fn validate_relative_fixture_path(path: &Path) {
    assert!(
        !path.as_os_str().is_empty(),
        "imported-global regression fixture path is empty"
    );
    assert!(
        !path.is_absolute(),
        "imported-global regression fixture path must be relative"
    );
    for component in path.components() {
        assert!(
            matches!(component, Component::Normal(_)),
            "imported-global regression fixture path must not escape its fixture directory: {path:?}"
        );
    }
    assert_eq!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("wat"),
        "imported-global regression fixture must be a .wat file: {path:?}"
    );
}

fn independent_expected(
    initial_state: i32,
    override_call: usize,
    override_value: i32,
    inputs: &[i32],
) -> TraceOutcome {
    assert!(override_call < inputs.len());
    let mut state = initial_state;
    let mut results = Vec::with_capacity(inputs.len());
    for (call, input) in inputs.iter().copied().enumerate() {
        if call == override_call {
            state = override_value;
        }
        state = state.wrapping_add(input);
        results.push(state);
    }
    TraceOutcome {
        results,
        final_state: state,
        failure_at: None,
    }
}

fn load_manifest() -> Vec<GlobalRegression> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/imported_global_regressions");
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
            9,
            "manifest line {line_number} must contain exactly nine tab-separated fields"
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
            behavior, "mutable_i32_global",
            "manifest line {line_number} has unsupported imported-global behavior {behavior:?}"
        );
        assert!(
            ids.insert(id.to_owned()),
            "duplicate imported-global regression id {id:?} on line {line_number}"
        );
        assert!(
            fixtures.insert(fixture.clone()),
            "duplicate imported-global regression fixture {fixture:?} on line {line_number}"
        );

        let initial_state = parse_i32(fields[3].trim(), "initial_state", line_number);
        let override_call = parse_usize(fields[4].trim(), "override_call", line_number);
        let override_value = parse_i32(fields[5].trim(), "override_value", line_number);
        let inputs = parse_i32_list(fields[6].trim(), "inputs", line_number);
        let expected_results = parse_i32_list(fields[7].trim(), "expected_results", line_number);
        let expected_final_state = parse_i32(fields[8].trim(), "expected_final_state", line_number);
        assert_eq!(
            inputs.len(),
            expected_results.len(),
            "manifest line {line_number} must have one expected result per input"
        );
        assert!(
            override_call < inputs.len(),
            "manifest line {line_number} override_call must point at an existing invocation"
        );

        let expected = TraceOutcome {
            results: expected_results,
            final_state: expected_final_state,
            failure_at: None,
        };
        assert_eq!(
            expected,
            independent_expected(initial_state, override_call, override_value, &inputs),
            "manifest line {line_number} disagrees with the independent mutable_i32_global model"
        );

        let full_path = root.join(&fixture);
        assert!(
            full_path.is_file(),
            "imported-global regression fixture does not exist: {full_path:?}"
        );
        regressions.push(GlobalRegression {
            id: id.to_owned(),
            fixture: full_path,
            initial_state,
            override_call,
            override_value,
            inputs,
            expected,
        });
    }

    assert!(
        !regressions.is_empty(),
        "imported-global regression manifest must contain at least one fixture"
    );
    regressions
}

fn run_mini(bytes: &[u8], regression: &GlobalRegression) -> TraceOutcome {
    let global = GlobalHandle::mutable(Value::I32(regression.initial_state));
    let mut hosts = HostRegistry::new();
    hosts
        .register_global("env", "g", global.clone())
        .expect("register imported-global regression backing");
    let module = parse_module(bytes).expect("imported-global regression fixture must parse");
    let mut instance = MiniInstance::with_hosts(module, hosts)
        .expect("imported-global regression fixture must instantiate");

    let mut results = Vec::with_capacity(regression.inputs.len());
    let mut failure_at = None;
    for (call, input) in regression.inputs.iter().copied().enumerate() {
        if call == regression.override_call {
            global
                .set(Value::I32(regression.override_value))
                .expect("override mini imported-global regression backing");
        }
        match instance.invoke_export_values("run", &[Value::I32(input)]) {
            Ok(values) => match values.as_slice() {
                [Value::I32(value)] => results.push(*value),
                other => panic!("unexpected mini imported-global replay result: {other:?}"),
            },
            Err(_) => {
                failure_at = Some(call);
                break;
            }
        }
    }
    let final_state = match global.get() {
        Value::I32(value) => value,
        other => panic!("unexpected mini imported-global replay backing: {other:?}"),
    };
    TraceOutcome {
        results,
        final_state,
        failure_at,
    }
}

fn run_reference(engine: &Engine, bytes: &[u8], regression: &GlobalRegression) -> TraceOutcome {
    let module = ReferenceModule::new(engine, bytes)
        .expect("imported-global regression fixture must compile in Wasmtime");
    let mut store = Store::new(engine, ());
    let global = Global::new(
        &mut store,
        GlobalType::new(ValType::I32, Mutability::Var),
        Val::I32(regression.initial_state),
    )
    .expect("create Wasmtime imported-global regression backing");
    let instance = ReferenceInstance::new(&mut store, &module, &[Extern::Global(global)])
        .expect("imported-global regression fixture must instantiate in Wasmtime");
    let run = instance
        .get_typed_func::<i32, i32>(&mut store, "run")
        .expect("imported-global regression run export must be [i32] -> [i32]");

    let mut results = Vec::with_capacity(regression.inputs.len());
    let mut failure_at = None;
    for (call, input) in regression.inputs.iter().copied().enumerate() {
        if call == regression.override_call {
            global
                .set(&mut store, Val::I32(regression.override_value))
                .expect("override Wasmtime imported-global regression backing");
        }
        match run.call(&mut store, input) {
            Ok(value) => results.push(value),
            Err(_) => {
                failure_at = Some(call);
                break;
            }
        }
    }
    let final_state = match global.get(&mut store) {
        Val::I32(value) => value,
        other => panic!("unexpected Wasmtime imported-global replay backing: {other:?}"),
    };
    TraceOutcome {
        results,
        final_state,
        failure_at,
    }
}

#[test]
fn imported_global_regressions_replay_against_wasmtime() {
    let regressions = load_manifest();
    let engine = Engine::default();

    for regression in regressions {
        let wat = fs::read_to_string(&regression.fixture).unwrap_or_else(|error| {
            panic!(
                "failed to read imported-global regression fixture {:?} for {}: {error}",
                regression.fixture, regression.id
            )
        });
        let bytes = wat::parse_str(&wat).unwrap_or_else(|error| {
            panic!(
                "failed to compile imported-global regression fixture {:?} for {}: {error}",
                regression.fixture, regression.id
            )
        });
        let mini = run_mini(&bytes, &regression);
        let reference = run_reference(&engine, &bytes, &regression);

        assert_eq!(
            mini, regression.expected,
            "mini imported-global regression replay failed for {}",
            regression.id
        );
        assert_eq!(
            reference, regression.expected,
            "Wasmtime imported-global regression expectation failed for {}",
            regression.id
        );
        assert_eq!(
            mini, reference,
            "cross-engine imported-global regression mismatch for {}",
            regression.id
        );
    }
}
