use std::{fs, path::PathBuf};

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

impl Mutation {
    fn as_str(self) -> &'static str {
        match self {
            Self::ClearOne => "clear1",
            Self::CopyZeroToOne => "copy0to1",
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::ClearOne => 0,
            Self::CopyZeroToOne => 1,
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct TableCase {
    mutation_call: usize,
    mutation: Mutation,
    addend: i32,
    xor_mask: i32,
    values: Vec<i32>,
    selectors: Vec<u32>,
}

impl TableCase {
    fn wat(&self) -> String {
        format!(
            "(module\n  (type $unary (func (param i32) (result i32)))\n  (import \"env\" \"tab\" (table 2 2 funcref))\n  (func $first (type $unary) (param i32) (result i32)\n    local.get 0\n    i32.const {}\n    i32.add)\n  (func $second (type $unary) (param i32) (result i32)\n    local.get 0\n    i32.const {}\n    i32.xor)\n  (elem (i32.const 0) $first $second)\n  (func (export \"run\") (param i32 i32) (result i32)\n    local.get 0\n    local.get 1\n    call_indirect (type $unary)))\n",
            self.addend, self.xor_mask
        )
    }

    fn expected(&self) -> TraceOutcome {
        assert_eq!(self.values.len(), self.selectors.len());
        assert!(!self.values.is_empty());
        assert!(self.mutation_call < self.values.len());

        #[derive(Clone, Copy)]
        enum SlotOne {
            First,
            Second,
            Null,
        }

        let mut slot_one = SlotOne::Second;
        let mut calls = Vec::with_capacity(self.values.len());
        for (call, (value, selector)) in self
            .values
            .iter()
            .copied()
            .zip(self.selectors.iter().copied())
            .enumerate()
        {
            if call == self.mutation_call {
                slot_one = match self.mutation {
                    Mutation::ClearOne => SlotOne::Null,
                    Mutation::CopyZeroToOne => SlotOne::First,
                };
            }

            let outcome = match selector {
                0 => CallOutcome::I32(value.wrapping_add(self.addend)),
                1 => match slot_one {
                    SlotOne::First => CallOutcome::I32(value.wrapping_add(self.addend)),
                    SlotOne::Second => CallOutcome::I32(value ^ self.xor_mask),
                    SlotOne::Null => CallOutcome::IndirectCallToNull,
                },
                _ => CallOutcome::TableOutOfBounds,
            };
            calls.push(outcome);
        }

        TraceOutcome {
            calls,
            final_slot_one_present: !matches!(slot_one, SlotOne::Null),
        }
    }
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

fn run_mini(bytes: &[u8], case: &TableCase) -> TraceOutcome {
    let table = TableHandle::new(2, Some(2)).expect("create mini imported table");
    let mut hosts = HostRegistry::new();
    hosts
        .register_table("env", "tab", table.clone())
        .expect("register mini imported table");
    let module = parse_module(bytes).expect("imported-table capture candidate must parse");
    let mut instance = MiniInstance::with_hosts(module, hosts)
        .expect("imported-table capture candidate must instantiate");

    let mut calls = Vec::with_capacity(case.values.len());
    for (call, (value, selector)) in case
        .values
        .iter()
        .copied()
        .zip(case.selectors.iter().copied())
        .enumerate()
    {
        if call == case.mutation_call {
            mutate_mini(&table, case.mutation);
        }
        let outcome = match instance
            .invoke_export_values("run", &[Value::I32(value), Value::I32(selector as i32)])
        {
            Ok(values) => match values.as_slice() {
                [Value::I32(value)] => CallOutcome::I32(*value),
                other => panic!("unexpected mini imported-table result shape: {other:?}"),
            },
            Err(RuntimeError::UninitializedTableElement(_)) => CallOutcome::IndirectCallToNull,
            Err(RuntimeError::TableElementOutOfBounds(_)) => CallOutcome::TableOutOfBounds,
            Err(error) => panic!("unmapped mini imported-table capture error: {error:?}"),
        };
        calls.push(outcome);
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

fn run_reference(engine: &Engine, bytes: &[u8], case: &TableCase) -> TraceOutcome {
    let module = ReferenceModule::new(engine, bytes)
        .expect("imported-table capture candidate must compile in Wasmtime");
    let mut store = Store::new(engine, ());
    let table = Table::new(
        &mut store,
        TableType::new(RefType::FUNCREF, 2, Some(2)),
        Ref::Func(None),
    )
    .expect("create Wasmtime imported table");
    let instance = ReferenceInstance::new(&mut store, &module, &[Extern::Table(table)])
        .expect("instantiate Wasmtime imported-table capture candidate");
    let run = instance
        .get_typed_func::<(i32, i32), i32>(&mut store, "run")
        .expect("imported-table run export must be [i32, i32] -> [i32]");

    let mut calls = Vec::with_capacity(case.values.len());
    for (call, (value, selector)) in case
        .values
        .iter()
        .copied()
        .zip(case.selectors.iter().copied())
        .enumerate()
    {
        if call == case.mutation_call {
            mutate_reference(&table, &mut store, case.mutation);
        }
        let outcome = match run.call(&mut store, (value, selector as i32)) {
            Ok(value) => CallOutcome::I32(value),
            Err(error) => match error.downcast_ref::<ReferenceTrap>() {
                Some(ReferenceTrap::IndirectCallToNull) => CallOutcome::IndirectCallToNull,
                Some(ReferenceTrap::TableOutOfBounds) => CallOutcome::TableOutOfBounds,
                Some(other) => panic!("unmapped Wasmtime imported-table trap: {other:?}"),
                None => panic!("Wasmtime imported-table error was not a trap: {error:?}"),
            },
        };
        calls.push(outcome);
    }

    TraceOutcome {
        calls,
        final_slot_one_present: reference_slot_one_present(&table, &mut store),
    }
}

fn observe(engine: &Engine, case: &TableCase) -> (TraceOutcome, TraceOutcome) {
    let bytes = wat::parse_str(case.wat()).expect("generated imported-table WAT must compile");
    (run_mini(&bytes, case), run_reference(engine, &bytes, case))
}

fn reproduces_reference_backed_mismatch(engine: &Engine, case: &TableCase) -> bool {
    let (mini, reference) = observe(engine, case);
    mini != reference && reference == case.expected()
}

type ValueRank = (u64, bool);
type StepRank = (u32, ValueRank);
type CaseRank = (usize, usize, u8, ValueRank, ValueRank, Vec<StepRank>);

fn value_rank(value: i32) -> ValueRank {
    (i64::from(value).unsigned_abs(), value.is_negative())
}

fn case_rank(case: &TableCase) -> CaseRank {
    (
        case.values.len(),
        case.mutation_call,
        case.mutation.rank(),
        value_rank(case.addend),
        value_rank(case.xor_mask),
        case.selectors
            .iter()
            .copied()
            .zip(case.values.iter().copied())
            .map(|(selector, value)| (selector, value_rank(value)))
            .collect(),
    )
}

fn value_candidates(value: i32) -> Vec<i32> {
    let original_rank = value_rank(value);
    let mut candidates = Vec::new();
    for candidate in [0_i32, 1, -1, 2, -2] {
        if value_rank(candidate) < original_rank && !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }
    let mut reduced = value;
    while reduced != 0 {
        reduced /= 2;
        if value_rank(reduced) < original_rank && !candidates.contains(&reduced) {
            candidates.push(reduced);
        }
    }
    candidates.sort_by_key(|candidate| value_rank(*candidate));
    candidates
}

fn selector_candidates(value: u32) -> Vec<u32> {
    let mut candidates = Vec::new();
    for candidate in [0_u32, 1, 2] {
        if candidate < value && !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }
    let mut reduced = value;
    while reduced != 0 {
        reduced /= 2;
        if reduced < value && !candidates.contains(&reduced) {
            candidates.push(reduced);
        }
    }
    candidates.sort_unstable();
    candidates
}

fn shrink_case(mut case: TableCase, mut reproduces: impl FnMut(&TableCase) -> bool) -> TableCase {
    assert!(
        reproduces(&case),
        "imported-table shrinker requires a reproducing input"
    );

    loop {
        let mut changed = false;

        for new_len in (case.mutation_call + 1)..case.values.len() {
            let mut candidate = case.clone();
            candidate.values.truncate(new_len);
            candidate.selectors.truncate(new_len);
            assert!(case_rank(&candidate) < case_rank(&case));
            if reproduces(&candidate) {
                case = candidate;
                changed = true;
                break;
            }
        }
        if changed {
            continue;
        }

        for mutation_call in 0..case.mutation_call {
            let mut candidate = case.clone();
            candidate.mutation_call = mutation_call;
            assert!(case_rank(&candidate) < case_rank(&case));
            if reproduces(&candidate) {
                case = candidate;
                changed = true;
                break;
            }
        }
        if changed {
            continue;
        }

        if case.mutation == Mutation::CopyZeroToOne {
            let mut candidate = case.clone();
            candidate.mutation = Mutation::ClearOne;
            assert!(case_rank(&candidate) < case_rank(&case));
            if reproduces(&candidate) {
                case = candidate;
                continue;
            }
        }

        for addend in value_candidates(case.addend) {
            let mut candidate = case.clone();
            candidate.addend = addend;
            assert!(case_rank(&candidate) < case_rank(&case));
            if reproduces(&candidate) {
                case = candidate;
                changed = true;
                break;
            }
        }
        if changed {
            continue;
        }

        for xor_mask in value_candidates(case.xor_mask) {
            let mut candidate = case.clone();
            candidate.xor_mask = xor_mask;
            assert!(case_rank(&candidate) < case_rank(&case));
            if reproduces(&candidate) {
                case = candidate;
                changed = true;
                break;
            }
        }
        if changed {
            continue;
        }

        'selectors: for index in 0..case.selectors.len() {
            for selector in selector_candidates(case.selectors[index]) {
                let mut candidate = case.clone();
                candidate.selectors[index] = selector;
                assert!(case_rank(&candidate) < case_rank(&case));
                if reproduces(&candidate) {
                    case = candidate;
                    changed = true;
                    break 'selectors;
                }
            }
        }
        if changed {
            continue;
        }

        'values: for index in 0..case.values.len() {
            for value in value_candidates(case.values[index]) {
                let mut candidate = case.clone();
                candidate.values[index] = value;
                assert!(case_rank(&candidate) < case_rank(&case));
                if reproduces(&candidate) {
                    case = candidate;
                    changed = true;
                    break 'values;
                }
            }
        }

        if !changed {
            return case;
        }
    }
}

#[derive(Debug)]
struct CaptureFiles {
    directory: PathBuf,
    driver_line: String,
}

fn capture_id(seed: u64, case_index: usize) -> String {
    format!("auto-import-table-{seed:016x}-{case_index:03}")
}

fn csv_i32(values: &[i32]) -> String {
    values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn csv_u32(values: &[u32]) -> String {
    values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn csv_outcomes(values: &[CallOutcome]) -> String {
    values
        .iter()
        .map(|outcome| match outcome {
            CallOutcome::I32(value) => format!("i32:{value}"),
            CallOutcome::IndirectCallToNull => "null".to_owned(),
            CallOutcome::TableOutOfBounds => "oob".to_owned(),
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn driver_line(id: &str, case: &TableCase) -> String {
    let expected = case.expected();
    format!(
        "{id}\t{id}.wat\tmutable_funcref_table\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        case.mutation_call,
        case.mutation.as_str(),
        case.addend,
        case.xor_mask,
        csv_i32(&case.values),
        csv_u32(&case.selectors),
        csv_outcomes(&expected.calls),
        if expected.final_slot_one_present {
            "present"
        } else {
            "null"
        }
    )
}

fn write_capture(
    seed: u64,
    case_index: usize,
    original: &TableCase,
    minimized: &TableCase,
    mini: &TraceOutcome,
    reference: &TraceOutcome,
) -> CaptureFiles {
    let id = capture_id(seed, case_index);
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("differential-captures");
    fs::create_dir_all(&directory).unwrap_or_else(|error| {
        panic!("failed to create imported-table capture directory {directory:?}: {error}")
    });

    let wat_path = directory.join(format!("{id}.wat"));
    fs::write(&wat_path, minimized.wat()).unwrap_or_else(|error| {
        panic!("failed to write minimized imported-table capture {wat_path:?}: {error}")
    });

    let driver_line = driver_line(&id, minimized);
    let driver_path = directory.join(format!("{id}.table.tsv"));
    fs::write(&driver_path, format!("{driver_line}\n")).unwrap_or_else(|error| {
        panic!("failed to write imported-table capture driver {driver_path:?}: {error}")
    });

    let metadata_path = directory.join(format!("{id}.txt"));
    let metadata = format!(
        "seed=0x{seed:016x}\ncase={case_index}\noriginal={original:?}\nminimized={minimized:?}\nmini={mini:?}\nreference={reference:?}\ndriver={driver_line}\n"
    );
    fs::write(&metadata_path, metadata).unwrap_or_else(|error| {
        panic!("failed to write imported-table capture metadata {metadata_path:?}: {error}")
    });

    CaptureFiles {
        directory,
        driver_line,
    }
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
fn imported_table_shrinker_reduces_trace_mutation_targets_and_inputs() {
    let original = TableCase {
        mutation_call: 2,
        mutation: Mutation::CopyZeroToOne,
        addend: 123_456,
        xor_mask: -654_321,
        values: vec![33, -44, 55, -66],
        selectors: vec![2, 1, 2, 1],
    };
    let minimized = shrink_case(original.clone(), |case| {
        case.values.len() >= 2
            && case.addend != 0
            && case.xor_mask != 0
            && case.values[0] != 0
            && case.values[1] != 0
    });
    assert_eq!(
        minimized,
        TableCase {
            mutation_call: 0,
            mutation: Mutation::ClearOne,
            addend: 1,
            xor_mask: 1,
            values: vec![1, 1],
            selectors: vec![0, 0],
        }
    );
    assert!(case_rank(&minimized) < case_rank(&original));
}

#[test]
fn imported_table_capture_renderer_emits_replay_compatible_driver() {
    let id = capture_id(0x0123_4567_89ab_cdef, 7);
    let case = TableCase {
        mutation_call: 1,
        mutation: Mutation::CopyZeroToOne,
        addend: 11,
        xor_mask: 85,
        values: vec![10, 20, 30],
        selectors: vec![1, 1, 2],
    };
    assert_eq!(id, "auto-import-table-0123456789abcdef-007");
    assert_eq!(
        driver_line(&id, &case),
        "auto-import-table-0123456789abcdef-007\tauto-import-table-0123456789abcdef-007.wat\tmutable_funcref_table\t1\tcopy0to1\t11\t85\t10,20,30\t1,1,2\ti32:95,i32:31,oob\tpresent"
    );
}

#[test]
fn generated_imported_table_differentials_capture_and_shrink_real_mismatches() {
    const SEED: u64 = 0x243f_6a88_85a3_08d3;
    const CASES: usize = 48;

    let engine = Engine::default();
    let mut rng = XorShift64::new(SEED);
    let mut successful = 0_usize;
    let mut null_traps = 0_usize;
    let mut oob_traps = 0_usize;
    let mut clear_cases = 0_usize;
    let mut copy_cases = 0_usize;

    for case_index in 0..CASES {
        let (mutation_call, mutation) = match case_index % 4 {
            0 => (1, Mutation::ClearOne),
            1 => (1, Mutation::CopyZeroToOne),
            2 => (3, Mutation::ClearOne),
            _ => (3, Mutation::CopyZeroToOne),
        };
        match mutation {
            Mutation::ClearOne => clear_cases += 1,
            Mutation::CopyZeroToOne => copy_cases += 1,
        }
        let addend = match case_index % 8 {
            0 => i32::MAX,
            1 => i32::MIN,
            _ => rng.next_i32(),
        };
        let xor_mask = match case_index % 8 {
            2 => -1,
            3 => 0,
            _ => rng.next_i32(),
        };
        let mut values = Vec::with_capacity(4);
        for call in 0..4 {
            values.push(match (case_index + call) % 11 {
                0 => i32::MAX,
                1 => i32::MIN,
                2 => -1,
                3 => 0,
                _ => rng.next_i32(),
            });
        }
        let case = TableCase {
            mutation_call,
            mutation,
            addend,
            xor_mask,
            values,
            selectors: vec![0, 1, 2, 1],
        };
        let expected = case.expected();
        for outcome in &expected.calls {
            match outcome {
                CallOutcome::I32(_) => successful += 1,
                CallOutcome::IndirectCallToNull => null_traps += 1,
                CallOutcome::TableOutOfBounds => oob_traps += 1,
            }
        }

        let (mini, reference) = observe(&engine, &case);
        if mini != reference {
            assert_eq!(
                reference, expected,
                "reference/model disagreement at seed={SEED:#018x} case={case_index}; refusing to capture an imported-table regression against an untrusted oracle"
            );
            let minimized = shrink_case(case.clone(), |candidate| {
                reproduces_reference_backed_mismatch(&engine, candidate)
            });
            let (minimized_mini, minimized_reference) = observe(&engine, &minimized);
            let capture = write_capture(
                SEED,
                case_index,
                &case,
                &minimized,
                &minimized_mini,
                &minimized_reference,
            );
            panic!(
                "imported-table differential mismatch at seed={SEED:#018x} case={case_index}: original={case:?}, minimized={minimized:?}, artifacts={:?}, driver={:?}",
                capture.directory, capture.driver_line
            );
        }

        assert_eq!(
            mini, expected,
            "mini/model imported-table mismatch at seed={SEED:#018x} case={case_index}: {case:?}"
        );
        assert_eq!(
            reference, expected,
            "Wasmtime/model imported-table mismatch at seed={SEED:#018x} case={case_index}: {case:?}"
        );
    }

    assert!(successful > 0, "imported-table corpus must execute calls");
    assert!(
        null_traps > 0,
        "imported-table corpus must execute null traps"
    );
    assert!(
        oob_traps > 0,
        "imported-table corpus must execute OOB traps"
    );
    assert!(
        clear_cases > 0,
        "imported-table corpus must clear host slots"
    );
    assert!(copy_cases > 0, "imported-table corpus must copy host slots");
}
