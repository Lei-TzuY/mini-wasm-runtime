use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
};

use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance as MiniInstance, RuntimeError, TableHandle, Value};
use wasmtime::{
    Engine, Extern, Instance as ReferenceInstance, Module as ReferenceModule, Ref, RefType, Store,
    Table, TableType, Trap as ReferenceTrap,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mutation {
    ClearOne,
    CopyZeroToOne,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallOutcome {
    I32(i32),
    IndirectCallToNull,
    TableOutOfBounds,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TraceOutcome {
    calls: Vec<CallOutcome>,
    final_slot_one_present: bool,
}

#[derive(Debug)]
struct ImportedTableRegression {
    id: String,
    fixture: PathBuf,
    mutation_call: usize,
    mutation: Mutation,
    addend: i32,
    xor_mask: i32,
    values: Vec<i32>,
    selectors: Vec<u32>,
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

fn parse_u32_list(value: &str, field: &str, line_number: usize) -> Vec<u32> {
    assert!(
        !value.is_empty(),
        "manifest line {line_number} has empty {field}"
    );
    value
        .split(',')
        .map(|item| parse_u32(item, field, line_number))
        .collect()
}

fn parse_outcomes(value: &str, line_number: usize) -> Vec<CallOutcome> {
    assert!(
        !value.is_empty(),
        "manifest line {line_number} has empty expected_outcomes"
    );
    value
        .split(',')
        .map(|item| {
            if let Some(value) = item.strip_prefix("i32:") {
                CallOutcome::I32(parse_i32(value, "expected i32 outcome", line_number))
            } else {
                match item {
                    "null" => CallOutcome::IndirectCallToNull,
                    "oob" => CallOutcome::TableOutOfBounds,
                    other => panic!(
                        "manifest line {line_number} has unsupported expected outcome {other:?}"
                    ),
                }
            }
        })
        .collect()
}

fn validate_relative_fixture_path(path: &Path) {
    assert!(
        !path.as_os_str().is_empty(),
        "imported-table regression fixture path is empty"
    );
    assert!(
        !path.is_absolute(),
        "imported-table regression fixture path must be relative"
    );
    for component in path.components() {
        assert!(
            matches!(component, Component::Normal(_)),
            "imported-table regression fixture path must not escape its fixture directory: {path:?}"
        );
    }
    assert_eq!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("wat"),
        "imported-table regression fixture must be a .wat file: {path:?}"
    );
}

fn independent_expected(regression: &ImportedTableRegression) -> TraceOutcome {
    #[derive(Clone, Copy)]
    enum SlotOne {
        First,
        Second,
        Null,
    }

    let mut slot_one = SlotOne::Second;
    let mut calls = Vec::with_capacity(regression.values.len());
    for (call, (value, selector)) in regression
        .values
        .iter()
        .copied()
        .zip(regression.selectors.iter().copied())
        .enumerate()
    {
        if call == regression.mutation_call {
            slot_one = match regression.mutation {
                Mutation::ClearOne => SlotOne::Null,
                Mutation::CopyZeroToOne => SlotOne::First,
            };
        }
        calls.push(match selector {
            0 => CallOutcome::I32(value.wrapping_add(regression.addend)),
            1 => match slot_one {
                SlotOne::First => CallOutcome::I32(value.wrapping_add(regression.addend)),
                SlotOne::Second => CallOutcome::I32(value ^ regression.xor_mask),
                SlotOne::Null => CallOutcome::IndirectCallToNull,
            },
            _ => CallOutcome::TableOutOfBounds,
        });
    }
    TraceOutcome {
        calls,
        final_slot_one_present: !matches!(slot_one, SlotOne::Null),
    }
}

fn load_manifest() -> Vec<ImportedTableRegression> {
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/imported_table_regressions");
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
            11,
            "manifest line {line_number} must contain exactly eleven tab-separated fields"
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
            behavior, "mutable_funcref_table",
            "manifest line {line_number} has unsupported behavior {behavior:?}"
        );
        assert!(
            ids.insert(id.to_owned()),
            "duplicate imported-table regression id {id:?} on line {line_number}"
        );
        assert!(
            fixtures.insert(fixture.clone()),
            "duplicate imported-table regression fixture {fixture:?} on line {line_number}"
        );

        let mutation_call = parse_usize(fields[3].trim(), "mutation_call", line_number);
        let mutation = match fields[4].trim() {
            "clear1" => Mutation::ClearOne,
            "copy0to1" => Mutation::CopyZeroToOne,
            other => panic!("manifest line {line_number} has unsupported mutation {other:?}"),
        };
        let addend = parse_i32(fields[5].trim(), "addend", line_number);
        let xor_mask = parse_i32(fields[6].trim(), "xor_mask", line_number);
        let values = parse_i32_list(fields[7].trim(), "values", line_number);
        let selectors = parse_u32_list(fields[8].trim(), "selectors", line_number);
        let expected_calls = parse_outcomes(fields[9].trim(), line_number);
        let final_slot_one_present = match fields[10].trim() {
            "present" => true,
            "null" => false,
            other => {
                panic!("manifest line {line_number} has invalid final_slot_one state {other:?}")
            }
        };

        assert_eq!(
            values.len(),
            selectors.len(),
            "manifest line {line_number} must have one selector per value"
        );
        assert_eq!(
            values.len(),
            expected_calls.len(),
            "manifest line {line_number} must have one expected outcome per call"
        );
        assert!(
            mutation_call < values.len(),
            "manifest line {line_number} mutation_call is outside the trace"
        );

        let expected = TraceOutcome {
            calls: expected_calls,
            final_slot_one_present,
        };
        let full_path = root.join(&fixture);
        assert!(
            full_path.is_file(),
            "imported-table regression fixture does not exist: {full_path:?}"
        );
        let regression = ImportedTableRegression {
            id: id.to_owned(),
            fixture: full_path,
            mutation_call,
            mutation,
            addend,
            xor_mask,
            values,
            selectors,
            expected,
        };
        assert_eq!(
            regression.expected,
            independent_expected(&regression),
            "manifest line {line_number} disagrees with the independent mutable_funcref_table model"
        );
        regressions.push(regression);
    }

    assert!(
        !regressions.is_empty(),
        "imported-table regression manifest must contain at least one fixture"
    );
    regressions
}

fn mutate_mini(table: &TableHandle, mutation: Mutation) {
    match mutation {
        Mutation::ClearOne => table
            .set(1, None)
            .expect("clear mini imported table slot 1"),
        Mutation::CopyZeroToOne => {
            let slot_zero = table
                .get(0)
                .expect("read mini imported table slot 0")
                .expect("element segment must initialize mini slot 0");
            table
                .set(1, Some(slot_zero))
                .expect("copy mini imported table slot 0 to slot 1");
        }
    }
}

fn run_mini(bytes: &[u8], regression: &ImportedTableRegression) -> TraceOutcome {
    let table = TableHandle::new(2, Some(2)).expect("create mini imported table");
    let mut hosts = HostRegistry::new();
    hosts
        .register_table("env", "tab", table.clone())
        .expect("register mini imported table");
    let module = parse_module(bytes).expect("imported-table regression fixture must parse");
    let mut instance = MiniInstance::with_hosts(module, hosts)
        .expect("imported-table regression fixture must instantiate");

    let mut calls = Vec::with_capacity(regression.values.len());
    for (call, (value, selector)) in regression
        .values
        .iter()
        .copied()
        .zip(regression.selectors.iter().copied())
        .enumerate()
    {
        if call == regression.mutation_call {
            mutate_mini(&table, regression.mutation);
        }
        calls.push(
            match instance
                .invoke_export_values("run", &[Value::I32(value), Value::I32(selector as i32)])
            {
                Ok(values) => match values.as_slice() {
                    [Value::I32(value)] => CallOutcome::I32(*value),
                    other => {
                        panic!("unexpected mini imported-table replay result shape: {other:?}")
                    }
                },
                Err(RuntimeError::UninitializedTableElement(_)) => CallOutcome::IndirectCallToNull,
                Err(RuntimeError::TableElementOutOfBounds(_)) => CallOutcome::TableOutOfBounds,
                Err(error) => panic!("unmapped mini imported-table replay error: {error:?}"),
            },
        );
    }

    TraceOutcome {
        calls,
        final_slot_one_present: table
            .get(1)
            .expect("read final mini imported table slot 1")
            .is_some(),
    }
}

fn mutate_reference(table: &Table, store: &mut Store<()>, mutation: Mutation) {
    match mutation {
        Mutation::ClearOne => table
            .set(store, 1, Ref::Func(None))
            .expect("clear Wasmtime imported table slot 1"),
        Mutation::CopyZeroToOne => {
            let slot_zero = table
                .get(&mut *store, 0)
                .expect("element segment must initialize Wasmtime slot 0");
            table
                .set(store, 1, slot_zero)
                .expect("copy Wasmtime imported table slot 0 to slot 1");
        }
    }
}

fn reference_slot_one_present(table: &Table, store: &mut Store<()>) -> bool {
    match table.get(store, 1) {
        Some(Ref::Func(value)) => value.is_some(),
        Some(other) => panic!("unexpected Wasmtime imported table ref: {other:?}"),
        None => panic!("Wasmtime imported table slot 1 disappeared"),
    }
}

fn run_reference(
    engine: &Engine,
    bytes: &[u8],
    regression: &ImportedTableRegression,
) -> TraceOutcome {
    let module = ReferenceModule::new(engine, bytes)
        .expect("imported-table regression fixture must compile in Wasmtime");
    let mut store = Store::new(engine, ());
    let table = Table::new(
        &mut store,
        TableType::new(RefType::FUNCREF, 2, Some(2)),
        Ref::Func(None),
    )
    .expect("create Wasmtime imported table");
    let instance = ReferenceInstance::new(&mut store, &module, &[Extern::Table(table)])
        .expect("instantiate Wasmtime imported-table regression fixture");
    let run = instance
        .get_typed_func::<(i32, i32), i32>(&mut store, "run")
        .expect("imported-table run export must be [i32, i32] -> [i32]");

    let mut calls = Vec::with_capacity(regression.values.len());
    for (call, (value, selector)) in regression
        .values
        .iter()
        .copied()
        .zip(regression.selectors.iter().copied())
        .enumerate()
    {
        if call == regression.mutation_call {
            mutate_reference(&table, &mut store, regression.mutation);
        }
        calls.push(match run.call(&mut store, (value, selector as i32)) {
            Ok(value) => CallOutcome::I32(value),
            Err(error) => match error.downcast_ref::<ReferenceTrap>() {
                Some(ReferenceTrap::IndirectCallToNull) => CallOutcome::IndirectCallToNull,
                Some(ReferenceTrap::TableOutOfBounds) => CallOutcome::TableOutOfBounds,
                Some(other) => panic!("unmapped Wasmtime imported-table replay trap: {other:?}"),
                None => panic!("Wasmtime imported-table replay error was not a trap: {error:?}"),
            },
        });
    }

    TraceOutcome {
        calls,
        final_slot_one_present: reference_slot_one_present(&table, &mut store),
    }
}

#[test]
fn imported_table_regression_fixtures_replay_against_wasmtime() {
    let regressions = load_manifest();
    let engine = Engine::default();

    for regression in regressions {
        let wat = fs::read_to_string(&regression.fixture).unwrap_or_else(|error| {
            panic!(
                "failed to read imported-table regression fixture {:?} for {}: {error}",
                regression.fixture, regression.id
            )
        });
        let bytes = wat::parse_str(&wat).unwrap_or_else(|error| {
            panic!(
                "failed to compile imported-table regression fixture {:?} for {}: {error}",
                regression.fixture, regression.id
            )
        });
        let mini = run_mini(&bytes, &regression);
        let reference = run_reference(&engine, &bytes, &regression);

        assert_eq!(
            mini, regression.expected,
            "mini runtime imported-table replay failed for {}",
            regression.id
        );
        assert_eq!(
            reference, regression.expected,
            "Wasmtime imported-table replay expectation failed for {}",
            regression.id
        );
        assert_eq!(
            mini, reference,
            "cross-engine imported-table regression mismatch for {}",
            regression.id
        );
    }
}
