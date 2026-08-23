use std::{fs, path::PathBuf};

use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, RuntimeError, Value};
use wasmtime::{
    Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store, Trap as ReferenceTrap,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableOutcome {
    I32(i32),
    TableOutOfBounds,
    IndirectCallToNull,
    ExecutionFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TableCase {
    selector: u32,
    second_initialized: bool,
    first: i32,
    second: i32,
}

impl TableCase {
    fn expected(self) -> TableOutcome {
        match self.selector {
            0 => TableOutcome::I32(self.first),
            1 if self.second_initialized => TableOutcome::I32(self.second),
            1 => TableOutcome::IndirectCallToNull,
            _ => TableOutcome::TableOutOfBounds,
        }
    }

    fn wat(self) -> String {
        let element = if self.second_initialized {
            "(elem (i32.const 0) $first $second)"
        } else {
            "(elem (i32.const 0) $first)"
        };
        format!(
            "(module\n  (type $ret (func (result i32)))\n  (table 2 funcref)\n  (func $first (type $ret) (result i32) i32.const {})\n  (func $second (type $ret) (result i32) i32.const {})\n  {}\n  (func (export \"run\") (result i32)\n    i32.const {}\n    call_indirect (type $ret)))\n",
            self.first, self.second, element, self.selector as i32
        )
    }
}

fn run_mini(bytes: &[u8]) -> TableOutcome {
    let module = parse_module(bytes).expect("table capture candidate must parse in mini runtime");
    let mut instance = MiniInstance::new(module)
        .expect("table capture candidate must validate and instantiate in mini runtime");
    match instance.invoke_export_values("run", &[]) {
        Ok(values) => match values.as_slice() {
            [Value::I32(value)] => TableOutcome::I32(*value),
            _ => TableOutcome::ExecutionFailure,
        },
        Err(RuntimeError::TableElementOutOfBounds(_)) => TableOutcome::TableOutOfBounds,
        Err(RuntimeError::UninitializedTableElement(_)) => TableOutcome::IndirectCallToNull,
        Err(_) => TableOutcome::ExecutionFailure,
    }
}

fn run_reference(engine: &Engine, bytes: &[u8]) -> TableOutcome {
    let module = ReferenceModule::new(engine, bytes).expect("table capture candidate must compile");
    let mut store = Store::new(engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("table capture candidate must instantiate in Wasmtime");
    let run = instance
        .get_typed_func::<(), i32>(&mut store, "run")
        .expect("table capture run export must be [] -> [i32]");
    match run.call(&mut store, ()) {
        Ok(value) => TableOutcome::I32(value),
        Err(error) => match error.downcast_ref::<ReferenceTrap>() {
            Some(ReferenceTrap::TableOutOfBounds) => TableOutcome::TableOutOfBounds,
            Some(ReferenceTrap::IndirectCallToNull) => TableOutcome::IndirectCallToNull,
            _ => TableOutcome::ExecutionFailure,
        },
    }
}

fn observe(engine: &Engine, case: TableCase) -> (TableOutcome, TableOutcome) {
    let bytes = wat::parse_str(case.wat()).expect("generated table capture WAT must compile");
    (run_mini(&bytes), run_reference(engine, &bytes))
}

fn reproduces_reference_backed_mismatch(engine: &Engine, case: TableCase) -> bool {
    let (mini, reference) = observe(engine, case);
    mini != reference && reference == case.expected()
}

fn i32_rank(value: i32) -> u128 {
    u128::from(i64::from(value).unsigned_abs()) * 2 + u128::from(value.is_negative())
}

fn case_rank(case: TableCase) -> [u128; 4] {
    [
        u128::from(case.selector),
        if case.second_initialized { 1 } else { 0 },
        i32_rank(case.first),
        i32_rank(case.second),
    ]
}

fn selector_candidates(value: u32) -> Vec<u32> {
    let mut candidates = Vec::new();
    for candidate in [0, 1, 2, 3] {
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

fn i32_candidates(value: i32) -> Vec<i32> {
    let original_rank = i32_rank(value);
    let mut candidates = Vec::new();

    for candidate in [0, 1, -1, 2, -2] {
        if i32_rank(candidate) < original_rank && !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }

    let mut reduced = value;
    while reduced != 0 {
        reduced /= 2;
        if i32_rank(reduced) < original_rank && !candidates.contains(&reduced) {
            candidates.push(reduced);
        }
    }

    candidates.sort_by_key(|candidate| i32_rank(*candidate));
    candidates
}

fn shrink_case(mut case: TableCase, mut reproduces: impl FnMut(TableCase) -> bool) -> TableCase {
    assert!(
        reproduces(case),
        "table shrinker requires a reproducing input"
    );

    loop {
        let mut changed = false;

        for selector in selector_candidates(case.selector) {
            let candidate = TableCase { selector, ..case };
            assert!(case_rank(candidate) < case_rank(case));
            if reproduces(candidate) {
                case = candidate;
                changed = true;
                break;
            }
        }
        if changed {
            continue;
        }

        if case.second_initialized {
            let candidate = TableCase {
                second_initialized: false,
                ..case
            };
            assert!(case_rank(candidate) < case_rank(case));
            if reproduces(candidate) {
                case = candidate;
                continue;
            }
        }

        for first in i32_candidates(case.first) {
            let candidate = TableCase { first, ..case };
            assert!(case_rank(candidate) < case_rank(case));
            if reproduces(candidate) {
                case = candidate;
                changed = true;
                break;
            }
        }
        if changed {
            continue;
        }

        for second in i32_candidates(case.second) {
            let candidate = TableCase { second, ..case };
            assert!(case_rank(candidate) < case_rank(case));
            if reproduces(candidate) {
                case = candidate;
                changed = true;
                break;
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
    manifest_line: String,
}

fn capture_id(seed: u64, case_index: usize) -> String {
    format!("auto-table-{seed:016x}-{case_index:03}")
}

fn manifest_line(id: &str, expected: TableOutcome) -> String {
    match expected {
        TableOutcome::I32(value) => format!("{id}\t{id}.wat\ti32\t{value}"),
        TableOutcome::TableOutOfBounds => {
            format!("{id}\t{id}.wat\ttrap\ttable_out_of_bounds")
        }
        TableOutcome::IndirectCallToNull => {
            format!("{id}\t{id}.wat\ttrap\tindirect_call_to_null")
        }
        TableOutcome::ExecutionFailure => {
            panic!("execution failure is not a promotable table expectation")
        }
    }
}

fn write_capture(
    seed: u64,
    case_index: usize,
    original: TableCase,
    minimized: TableCase,
    mini: TableOutcome,
    reference: TableOutcome,
) -> CaptureFiles {
    let id = capture_id(seed, case_index);
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("differential-captures");
    fs::create_dir_all(&directory).unwrap_or_else(|error| {
        panic!("failed to create table capture directory {directory:?}: {error}")
    });

    let wat_path = directory.join(format!("{id}.wat"));
    fs::write(&wat_path, minimized.wat()).unwrap_or_else(|error| {
        panic!("failed to write minimized table capture {wat_path:?}: {error}")
    });

    let manifest_line = manifest_line(&id, minimized.expected());
    let manifest_path = directory.join(format!("{id}.manifest.tsv"));
    fs::write(&manifest_path, format!("{manifest_line}\n")).unwrap_or_else(|error| {
        panic!("failed to write table capture manifest {manifest_path:?}: {error}")
    });

    let metadata_path = directory.join(format!("{id}.txt"));
    let metadata = format!(
        "seed=0x{seed:016x}\ncase={case_index}\noriginal={original:?}\nminimized={minimized:?}\nmini={mini:?}\nreference={reference:?}\nmanifest={manifest_line}\n"
    );
    fs::write(&metadata_path, metadata).unwrap_or_else(|error| {
        panic!("failed to write table capture metadata {metadata_path:?}: {error}")
    });

    CaptureFiles {
        directory,
        manifest_line,
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
fn table_shrinker_reduces_selector_and_target_values_monotonically() {
    let original = TableCase {
        selector: 9,
        second_initialized: true,
        first: 123_456,
        second: -654_321,
    };
    let minimized = shrink_case(original, |case| {
        case.selector >= 1 && case.second_initialized && case.first != 0 && case.second != 0
    });
    assert_eq!(
        minimized,
        TableCase {
            selector: 1,
            second_initialized: true,
            first: 1,
            second: 1,
        }
    );
    assert!(case_rank(minimized) < case_rank(original));
}

#[test]
fn table_shrinker_can_remove_an_unneeded_element_initializer() {
    let original = TableCase {
        selector: 1,
        second_initialized: true,
        first: 99,
        second: 123,
    };
    let minimized = shrink_case(original, |case| case.selector == 1 && case.first != 0);
    assert_eq!(
        minimized,
        TableCase {
            selector: 1,
            second_initialized: false,
            first: 1,
            second: 0,
        }
    );
}

#[test]
fn table_capture_renderer_emits_replay_compatible_payloads() {
    let id = capture_id(0x0123_4567_89ab_cdef, 13);
    assert_eq!(id, "auto-table-0123456789abcdef-013");
    assert_eq!(
        manifest_line(&id, TableOutcome::I32(42)),
        "auto-table-0123456789abcdef-013\tauto-table-0123456789abcdef-013.wat\ti32\t42"
    );
    assert_eq!(
        manifest_line(&id, TableOutcome::TableOutOfBounds),
        "auto-table-0123456789abcdef-013\tauto-table-0123456789abcdef-013.wat\ttrap\ttable_out_of_bounds"
    );
    assert_eq!(
        manifest_line(&id, TableOutcome::IndirectCallToNull),
        "auto-table-0123456789abcdef-013\tauto-table-0123456789abcdef-013.wat\ttrap\tindirect_call_to_null"
    );
}

#[test]
fn generated_table_differentials_capture_and_shrink_real_mismatches() {
    const SEED: u64 = 0xd1b5_4a32_d192_ed03;
    const CASES: usize = 96;

    let engine = Engine::default();
    let mut rng = XorShift64::new(SEED);
    let mut successful = 0_usize;
    let mut null_traps = 0_usize;
    let mut oob_traps = 0_usize;

    for case_index in 0..CASES {
        let (selector, second_initialized) = match case_index % 4 {
            0 => (0, rng.next_u64() & 1 != 0),
            1 => (1, true),
            2 => (1, false),
            _ => (2 + (rng.next_u64() % 6) as u32, rng.next_u64() & 1 != 0),
        };
        let case = TableCase {
            selector,
            second_initialized,
            first: rng.next_i32(),
            second: rng.next_i32(),
        };
        let expected = case.expected();
        match expected {
            TableOutcome::I32(_) => successful += 1,
            TableOutcome::IndirectCallToNull => null_traps += 1,
            TableOutcome::TableOutOfBounds => oob_traps += 1,
            TableOutcome::ExecutionFailure => unreachable!(),
        }

        let (mini, reference) = observe(&engine, case);
        if mini != reference {
            assert_eq!(
                reference, expected,
                "reference/model disagreement at seed={SEED:#018x} case={case_index}; refusing to capture a table regression against an untrusted oracle"
            );
            let minimized = shrink_case(case, |candidate| {
                reproduces_reference_backed_mismatch(&engine, candidate)
            });
            let (minimized_mini, minimized_reference) = observe(&engine, minimized);
            let capture = write_capture(
                SEED,
                case_index,
                case,
                minimized,
                minimized_mini,
                minimized_reference,
            );
            panic!(
                "table differential mismatch at seed={SEED:#018x} case={case_index}: original={case:?}, minimized={minimized:?}, artifacts={:?}, manifest_row={:?}",
                capture.directory, capture.manifest_line
            );
        }

        assert_eq!(
            mini, expected,
            "mini/model table mismatch at seed={SEED:#018x} case={case_index}: {case:?}"
        );
        assert_eq!(
            reference, expected,
            "Wasmtime/model table mismatch at seed={SEED:#018x} case={case_index}: {case:?}"
        );
    }

    assert!(successful > 0, "table corpus must exercise successful calls");
    assert!(null_traps > 0, "table corpus must exercise null indirect calls");
    assert!(oob_traps > 0, "table corpus must exercise table OOB traps");
}
