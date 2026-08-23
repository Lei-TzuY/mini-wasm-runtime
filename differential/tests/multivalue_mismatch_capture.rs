use std::{fs, path::PathBuf};

use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MultiOutcome {
    Pair(i32, i64),
    ExecutionFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MultiCase {
    condition: i32,
    then_i32: i32,
    then_i64: i64,
    else_i32: i32,
    else_i64: i64,
}

impl MultiCase {
    fn expected(self) -> MultiOutcome {
        if self.condition != 0 {
            MultiOutcome::Pair(self.then_i32, self.then_i64)
        } else {
            MultiOutcome::Pair(self.else_i32, self.else_i64)
        }
    }

    fn wat(self) -> String {
        format!(
            "(module\n  (func (export \"run\") (result i32 i64)\n    i32.const {}\n    if (result i32 i64)\n      i32.const {}\n      i64.const {}\n    else\n      i32.const {}\n      i64.const {}\n    end))\n",
            self.condition, self.then_i32, self.then_i64, self.else_i32, self.else_i64
        )
    }
}

fn run_mini(bytes: &[u8]) -> MultiOutcome {
    let module = parse_module(bytes).expect("multi-value capture candidate must parse");
    let mut instance = MiniInstance::new(module)
        .expect("multi-value capture candidate must validate and instantiate");
    match instance.invoke_export_values("run", &[]) {
        Ok(values) => match values.as_slice() {
            [Value::I32(first), Value::I64(second)] => MultiOutcome::Pair(*first, *second),
            _ => MultiOutcome::ExecutionFailure,
        },
        Err(_) => MultiOutcome::ExecutionFailure,
    }
}

fn run_reference(engine: &Engine, bytes: &[u8]) -> MultiOutcome {
    let module =
        ReferenceModule::new(engine, bytes).expect("multi-value capture candidate must compile");
    let mut store = Store::new(engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("multi-value capture candidate must instantiate in Wasmtime");
    let run = instance
        .get_typed_func::<(), (i32, i64)>(&mut store, "run")
        .expect("multi-value capture run export must be [] -> [i32, i64]");
    match run.call(&mut store, ()) {
        Ok((first, second)) => MultiOutcome::Pair(first, second),
        Err(_) => MultiOutcome::ExecutionFailure,
    }
}

fn observe(engine: &Engine, case: MultiCase) -> (MultiOutcome, MultiOutcome) {
    let bytes = wat::parse_str(case.wat()).expect("generated multi-value capture WAT must compile");
    (run_mini(&bytes), run_reference(engine, &bytes))
}

fn reproduces_reference_backed_mismatch(engine: &Engine, case: MultiCase) -> bool {
    let (mini, reference) = observe(engine, case);
    mini != reference && reference == case.expected()
}

fn i32_rank(value: i32) -> u128 {
    u128::from(i64::from(value).unsigned_abs()) * 2 + u128::from(value.is_negative())
}

fn i64_rank(value: i64) -> u128 {
    u128::from(value.unsigned_abs()) * 2 + u128::from(value.is_negative())
}

fn case_rank(case: MultiCase) -> [u128; 5] {
    [
        i32_rank(case.condition),
        i32_rank(case.then_i32),
        i64_rank(case.then_i64),
        i32_rank(case.else_i32),
        i64_rank(case.else_i64),
    ]
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

fn i64_candidates(value: i64) -> Vec<i64> {
    let original_rank = i64_rank(value);
    let mut candidates = Vec::new();

    for candidate in [0_i64, 1, -1, 2, -2] {
        if i64_rank(candidate) < original_rank && !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }

    let mut reduced = value;
    while reduced != 0 {
        reduced /= 2;
        if i64_rank(reduced) < original_rank && !candidates.contains(&reduced) {
            candidates.push(reduced);
        }
    }

    candidates.sort_by_key(|candidate| i64_rank(*candidate));
    candidates
}

fn shrink_case(mut case: MultiCase, mut reproduces: impl FnMut(MultiCase) -> bool) -> MultiCase {
    assert!(
        reproduces(case),
        "multi-value shrinker requires a reproducing input"
    );

    loop {
        let mut changed = false;

        for condition in i32_candidates(case.condition) {
            let candidate = MultiCase { condition, ..case };
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

        for then_i32 in i32_candidates(case.then_i32) {
            let candidate = MultiCase { then_i32, ..case };
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

        for then_i64 in i64_candidates(case.then_i64) {
            let candidate = MultiCase { then_i64, ..case };
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

        for else_i32 in i32_candidates(case.else_i32) {
            let candidate = MultiCase { else_i32, ..case };
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

        for else_i64 in i64_candidates(case.else_i64) {
            let candidate = MultiCase { else_i64, ..case };
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
    format!("auto-multivalue-{seed:016x}-{case_index:03}")
}

fn manifest_line(id: &str, expected: MultiOutcome) -> String {
    match expected {
        MultiOutcome::Pair(first, second) => {
            format!("{id}\t{id}.wat\tpair_i32_i64\t{first},{second}")
        }
        MultiOutcome::ExecutionFailure => {
            panic!("execution failure is not a promotable multi-value expectation")
        }
    }
}

fn write_capture(
    seed: u64,
    case_index: usize,
    original: MultiCase,
    minimized: MultiCase,
    mini: MultiOutcome,
    reference: MultiOutcome,
) -> CaptureFiles {
    let id = capture_id(seed, case_index);
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("differential-captures");
    fs::create_dir_all(&directory).unwrap_or_else(|error| {
        panic!("failed to create multi-value capture directory {directory:?}: {error}")
    });

    let wat_path = directory.join(format!("{id}.wat"));
    fs::write(&wat_path, minimized.wat()).unwrap_or_else(|error| {
        panic!("failed to write minimized multi-value capture {wat_path:?}: {error}")
    });

    let manifest_line = manifest_line(&id, minimized.expected());
    let manifest_path = directory.join(format!("{id}.manifest.tsv"));
    fs::write(&manifest_path, format!("{manifest_line}\n")).unwrap_or_else(|error| {
        panic!("failed to write multi-value capture manifest {manifest_path:?}: {error}")
    });

    let metadata_path = directory.join(format!("{id}.txt"));
    let metadata = format!(
        "seed=0x{seed:016x}\ncase={case_index}\noriginal={original:?}\nminimized={minimized:?}\nmini={mini:?}\nreference={reference:?}\nmanifest={manifest_line}\n"
    );
    fs::write(&metadata_path, metadata).unwrap_or_else(|error| {
        panic!("failed to write multi-value capture metadata {metadata_path:?}: {error}")
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

    fn next_i64(&mut self) -> i64 {
        self.next_u64() as i64
    }
}

#[test]
fn multi_value_shrinker_reduces_both_active_and_inactive_branches() {
    let original = MultiCase {
        condition: 123,
        then_i32: 456,
        then_i64: 789,
        else_i32: -321,
        else_i64: -654,
    };
    let minimized = shrink_case(original, |case| {
        case.condition != 0 && case.then_i32 != 0 && case.then_i64 != 0
    });
    assert_eq!(
        minimized,
        MultiCase {
            condition: 1,
            then_i32: 1,
            then_i64: 1,
            else_i32: 0,
            else_i64: 0,
        }
    );
    assert!(case_rank(minimized) < case_rank(original));
}

#[test]
fn multi_value_capture_renderer_emits_replay_compatible_payload() {
    let id = capture_id(0x0123_4567_89ab_cdef, 11);
    assert_eq!(id, "auto-multivalue-0123456789abcdef-011");
    assert_eq!(
        manifest_line(&id, MultiOutcome::Pair(7, 9_000_000_000)),
        "auto-multivalue-0123456789abcdef-011\tauto-multivalue-0123456789abcdef-011.wat\tpair_i32_i64\t7,9000000000"
    );
}

#[test]
fn generated_multi_value_differentials_capture_and_shrink_real_mismatches() {
    const SEED: u64 = 0x510e_527f_ade6_82d1;
    const CASES: usize = 96;

    let engine = Engine::default();
    let mut rng = XorShift64::new(SEED);
    let mut then_cases = 0_usize;
    let mut else_cases = 0_usize;

    for case_index in 0..CASES {
        let raw_condition = rng.next_i32();
        let condition = if case_index % 3 == 0 {
            0
        } else if raw_condition == 0 {
            1
        } else {
            raw_condition
        };
        let case = MultiCase {
            condition,
            then_i32: rng.next_i32(),
            then_i64: rng.next_i64(),
            else_i32: rng.next_i32(),
            else_i64: rng.next_i64(),
        };
        if condition == 0 {
            else_cases += 1;
        } else {
            then_cases += 1;
        }

        let expected = case.expected();
        let (mini, reference) = observe(&engine, case);
        if mini != reference {
            assert_eq!(
                reference, expected,
                "reference/model disagreement at seed={SEED:#018x} case={case_index}; refusing to capture a multi-value regression against an untrusted oracle"
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
                "multi-value differential mismatch at seed={SEED:#018x} case={case_index}: original={case:?}, minimized={minimized:?}, artifacts={:?}, manifest_row={:?}",
                capture.directory, capture.manifest_line
            );
        }

        assert_eq!(
            mini, expected,
            "mini/model multi-value mismatch at seed={SEED:#018x} case={case_index}: {case:?}"
        );
        assert_eq!(
            reference, expected,
            "Wasmtime/model multi-value mismatch at seed={SEED:#018x} case={case_index}: {case:?}"
        );
    }

    assert!(then_cases > 0, "multi-value corpus must exercise then branches");
    assert!(else_cases > 0, "multi-value corpus must exercise else branches");
}
