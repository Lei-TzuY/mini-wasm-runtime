use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
};

use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance as MiniInstance, MemoryHandle, Value};
use wasmtime::{
    Engine, Extern, Instance as ReferenceInstance, Memory, MemoryType, Module as ReferenceModule,
    Store,
};

const PAGE_BYTES: u32 = 65_536;
const WIDTH: u32 = 4;
const LAST_VALID_I32_ADDRESS: u32 = PAGE_BYTES - WIDTH;

#[derive(Debug, Clone, PartialEq, Eq)]
struct TraceOutcome {
    results: Vec<i32>,
    final_value: i32,
    failure_at: Option<usize>,
}

#[derive(Debug)]
struct MemoryRegression {
    id: String,
    fixture: PathBuf,
    address: u32,
    initial_value: i32,
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

fn parse_u32(value: &str, field: &str, line_number: usize) -> u32 {
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
        "imported-memory regression fixture path is empty"
    );
    assert!(
        !path.is_absolute(),
        "imported-memory regression fixture path must be relative"
    );
    for component in path.components() {
        assert!(
            matches!(component, Component::Normal(_)),
            "imported-memory regression fixture path must not escape its fixture directory: {path:?}"
        );
    }
    assert_eq!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("wat"),
        "imported-memory regression fixture must be a .wat file: {path:?}"
    );
}

fn independent_expected(
    initial_value: i32,
    override_call: usize,
    override_value: i32,
    inputs: &[i32],
) -> TraceOutcome {
    assert!(override_call < inputs.len());
    let mut value = initial_value;
    let mut results = Vec::with_capacity(inputs.len());
    for (call, input) in inputs.iter().copied().enumerate() {
        if call == override_call {
            value = override_value;
        }
        value = value.wrapping_add(input);
        results.push(value);
    }
    TraceOutcome {
        results,
        final_value: value,
        failure_at: None,
    }
}

fn load_manifest() -> Vec<MemoryRegression> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/imported_memory_regressions");
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
            10,
            "manifest line {line_number} must contain exactly ten tab-separated fields"
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
            behavior, "mutable_i32_memory",
            "manifest line {line_number} has unsupported imported-memory behavior {behavior:?}"
        );
        assert!(
            ids.insert(id.to_owned()),
            "duplicate imported-memory regression id {id:?} on line {line_number}"
        );
        assert!(
            fixtures.insert(fixture.clone()),
            "duplicate imported-memory regression fixture {fixture:?} on line {line_number}"
        );

        let address = parse_u32(fields[3].trim(), "address", line_number);
        assert!(
            address <= LAST_VALID_I32_ADDRESS,
            "manifest line {line_number} address {address} cannot fit one i32 in a one-page memory"
        );
        let initial_value = parse_i32(fields[4].trim(), "initial_value", line_number);
        let override_call = parse_usize(fields[5].trim(), "override_call", line_number);
        let override_value = parse_i32(fields[6].trim(), "override_value", line_number);
        let inputs = parse_i32_list(fields[7].trim(), "inputs", line_number);
        let expected_results = parse_i32_list(fields[8].trim(), "expected_results", line_number);
        let expected_final_value = parse_i32(fields[9].trim(), "expected_final_value", line_number);
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
            final_value: expected_final_value,
            failure_at: None,
        };
        assert_eq!(
            expected,
            independent_expected(initial_value, override_call, override_value, &inputs),
            "manifest line {line_number} disagrees with the independent mutable_i32_memory model"
        );

        let full_path = root.join(&fixture);
        assert!(
            full_path.is_file(),
            "imported-memory regression fixture does not exist: {full_path:?}"
        );
        regressions.push(MemoryRegression {
            id: id.to_owned(),
            fixture: full_path,
            address,
            initial_value,
            override_call,
            override_value,
            inputs,
            expected,
        });
    }

    assert!(
        !regressions.is_empty(),
        "imported-memory regression manifest must contain at least one fixture"
    );
    regressions
}

fn read_mini_i32(memory: &MemoryHandle, address: u32) -> i32 {
    let bytes = memory
        .read(address, WIDTH as usize)
        .expect("read imported-memory regression backing");
    i32::from_le_bytes(
        bytes
            .try_into()
            .expect("four-byte imported-memory replay read"),
    )
}

fn run_mini(bytes: &[u8], regression: &MemoryRegression) -> TraceOutcome {
    let memory = MemoryHandle::new(1, Some(2)).expect("create imported-memory regression backing");
    memory
        .write(regression.address, &regression.initial_value.to_le_bytes())
        .expect("seed imported-memory regression backing");
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "mem", memory.clone())
        .expect("register imported-memory regression backing");
    let module = parse_module(bytes).expect("imported-memory regression fixture must parse");
    let mut instance = MiniInstance::with_hosts(module, hosts)
        .expect("imported-memory regression fixture must instantiate");

    let mut results = Vec::with_capacity(regression.inputs.len());
    let mut failure_at = None;
    for (call, input) in regression.inputs.iter().copied().enumerate() {
        if call == regression.override_call {
            memory
                .write(regression.address, &regression.override_value.to_le_bytes())
                .expect("override mini imported-memory regression backing");
        }
        match instance.invoke_export_values("run", &[Value::I32(input)]) {
            Ok(values) => match values.as_slice() {
                [Value::I32(value)] => results.push(*value),
                other => panic!("unexpected mini imported-memory replay result: {other:?}"),
            },
            Err(_) => {
                failure_at = Some(call);
                break;
            }
        }
    }

    TraceOutcome {
        results,
        final_value: read_mini_i32(&memory, regression.address),
        failure_at,
    }
}

fn read_reference_i32(memory: Memory, store: &Store<()>, address: u32) -> i32 {
    let mut bytes = [0_u8; WIDTH as usize];
    memory
        .read(store, address as usize, &mut bytes)
        .expect("read Wasmtime imported-memory regression backing");
    i32::from_le_bytes(bytes)
}

fn run_reference(engine: &Engine, bytes: &[u8], regression: &MemoryRegression) -> TraceOutcome {
    let module = ReferenceModule::new(engine, bytes)
        .expect("imported-memory regression fixture must compile in Wasmtime");
    let mut store = Store::new(engine, ());
    let memory = Memory::new(&mut store, MemoryType::new(1, Some(2)))
        .expect("create Wasmtime imported-memory regression backing");
    memory
        .write(
            &mut store,
            regression.address as usize,
            &regression.initial_value.to_le_bytes(),
        )
        .expect("seed Wasmtime imported-memory regression backing");
    let instance = ReferenceInstance::new(&mut store, &module, &[Extern::Memory(memory)])
        .expect("imported-memory regression fixture must instantiate in Wasmtime");
    let run = instance
        .get_typed_func::<i32, i32>(&mut store, "run")
        .expect("imported-memory regression run export must be [i32] -> [i32]");

    let mut results = Vec::with_capacity(regression.inputs.len());
    let mut failure_at = None;
    for (call, input) in regression.inputs.iter().copied().enumerate() {
        if call == regression.override_call {
            memory
                .write(
                    &mut store,
                    regression.address as usize,
                    &regression.override_value.to_le_bytes(),
                )
                .expect("override Wasmtime imported-memory regression backing");
        }
        match run.call(&mut store, input) {
            Ok(value) => results.push(value),
            Err(_) => {
                failure_at = Some(call);
                break;
            }
        }
    }

    TraceOutcome {
        results,
        final_value: read_reference_i32(memory, &store, regression.address),
        failure_at,
    }
}

#[test]
fn imported_memory_regressions_replay_against_wasmtime() {
    let regressions = load_manifest();
    let engine = Engine::default();

    for regression in regressions {
        let wat = fs::read_to_string(&regression.fixture).unwrap_or_else(|error| {
            panic!(
                "failed to read imported-memory regression fixture {:?} for {}: {error}",
                regression.fixture, regression.id
            )
        });
        let bytes = wat::parse_str(&wat).unwrap_or_else(|error| {
            panic!(
                "failed to compile imported-memory regression fixture {:?} for {}: {error}",
                regression.fixture, regression.id
            )
        });
        let mini = run_mini(&bytes, &regression);
        let reference = run_reference(&engine, &bytes, &regression);

        assert_eq!(
            mini, regression.expected,
            "mini imported-memory regression replay failed for {}",
            regression.id
        );
        assert_eq!(
            reference, regression.expected,
            "Wasmtime imported-memory regression expectation failed for {}",
            regression.id
        );
        assert_eq!(
            mini, reference,
            "cross-engine imported-memory regression mismatch for {}",
            regression.id
        );
    }
}
