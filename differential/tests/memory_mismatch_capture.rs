use std::{fs, path::PathBuf};

use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, RuntimeError, Value};
use wasmtime::{
    Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store, Trap as ReferenceTrap,
};

const PAGE_BYTES: u64 = 65_536;
const WIDTH: u64 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemoryOutcome {
    I32(i32),
    MemoryOutOfBounds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MemoryCase {
    address: u32,
    offset: u32,
    value: i32,
}

impl MemoryCase {
    fn expected(self) -> MemoryOutcome {
        let effective = u64::from(self.address) + u64::from(self.offset);
        if effective + WIDTH <= PAGE_BYTES {
            MemoryOutcome::I32(self.value)
        } else {
            MemoryOutcome::MemoryOutOfBounds
        }
    }

    fn wat(self) -> String {
        format!(
            "(module\n  (memory 1)\n  (func (export \"run\") (result i32)\n    i32.const {}\n    i32.const {}\n    i32.store offset={}\n    i32.const {}\n    i32.load offset={}))\n",
            self.address as i32, self.value, self.offset, self.address as i32, self.offset
        )
    }
}

fn run_mini(bytes: &[u8]) -> MemoryOutcome {
    let module = parse_module(bytes).expect("memory capture candidate must parse in mini runtime");
    let mut instance = MiniInstance::new(module)
        .expect("memory capture candidate must validate and instantiate in mini runtime");
    match instance.invoke_export_values("run", &[]) {
        Ok(values) => match values.as_slice() {
            [Value::I32(value)] => MemoryOutcome::I32(*value),
            other => panic!("unexpected mini memory-capture result shape: {other:?}"),
        },
        Err(RuntimeError::MemoryOutOfBounds { .. }) => MemoryOutcome::MemoryOutOfBounds,
        Err(error) => panic!("unmapped mini memory-capture error: {error:?}"),
    }
}

fn run_reference(engine: &Engine, bytes: &[u8]) -> MemoryOutcome {
    let module =
        ReferenceModule::new(engine, bytes).expect("memory capture candidate must compile");
    let mut store = Store::new(engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("memory capture candidate must instantiate in Wasmtime");
    let run = instance
        .get_typed_func::<(), i32>(&mut store, "run")
        .expect("memory capture run export must be [] -> [i32]");
    match run.call(&mut store, ()) {
        Ok(value) => MemoryOutcome::I32(value),
        Err(error) => match error.downcast_ref::<ReferenceTrap>() {
            Some(ReferenceTrap::MemoryOutOfBounds) => MemoryOutcome::MemoryOutOfBounds,
            Some(other) => panic!("unmapped Wasmtime memory-capture trap: {other:?}"),
            None => panic!("Wasmtime memory-capture error was not a trap: {error:?}"),
        },
    }
}

fn observe(engine: &Engine, case: MemoryCase) -> (MemoryOutcome, MemoryOutcome) {
    let bytes = wat::parse_str(case.wat()).expect("generated memory-capture WAT must compile");
    (run_mini(&bytes), run_reference(engine, &bytes))
}

fn reproduces_reference_backed_mismatch(engine: &Engine, case: MemoryCase) -> bool {
    let (mini, reference) = observe(engine, case);
    mini != reference && reference == case.expected()
}

fn value_rank(value: i32) -> (u64, bool) {
    (i64::from(value).unsigned_abs(), value.is_negative())
}

fn case_rank(case: MemoryCase) -> (u32, u32, u64, bool) {
    let value = value_rank(case.value);
    (case.address, case.offset, value.0, value.1)
}

fn u32_candidates(value: u32, boundary_aware: bool) -> Vec<u32> {
    let mut candidates = Vec::new();
    let boundary_values: &[u32] = if boundary_aware {
        &[0, 1, 2, 3, 4, 65_532, 65_533, 65_535, 65_536]
    } else {
        &[0, 1, 2, 3, 4, 8, 16, 32, 64, 128, 256]
    };

    for candidate in boundary_values {
        if *candidate < value && !candidates.contains(candidate) {
            candidates.push(*candidate);
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
    let original_rank = value_rank(value);
    let mut candidates = Vec::new();

    for candidate in [0, 1, -1, 2, -2] {
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

fn shrink_case(mut case: MemoryCase, mut reproduces: impl FnMut(MemoryCase) -> bool) -> MemoryCase {
    assert!(
        reproduces(case),
        "memory shrinker requires a reproducing input"
    );

    loop {
        let mut changed = false;

        for address in u32_candidates(case.address, true) {
            let candidate = MemoryCase { address, ..case };
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

        for offset in u32_candidates(case.offset, false) {
            let candidate = MemoryCase { offset, ..case };
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

        for value in i32_candidates(case.value) {
            let candidate = MemoryCase { value, ..case };
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
    format!("auto-memory-{seed:016x}-{case_index:03}")
}

fn manifest_line(id: &str, expected: MemoryOutcome) -> String {
    match expected {
        MemoryOutcome::I32(value) => format!("{id}\t{id}.wat\ti32\t{value}"),
        MemoryOutcome::MemoryOutOfBounds => {
            format!("{id}\t{id}.wat\ttrap\tmemory_out_of_bounds")
        }
    }
}

fn write_capture(
    seed: u64,
    case_index: usize,
    original: MemoryCase,
    minimized: MemoryCase,
    mini: MemoryOutcome,
    reference: MemoryOutcome,
) -> CaptureFiles {
    let id = capture_id(seed, case_index);
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("differential-captures");
    fs::create_dir_all(&directory).unwrap_or_else(|error| {
        panic!("failed to create memory-capture directory {directory:?}: {error}")
    });

    let wat_path = directory.join(format!("{id}.wat"));
    fs::write(&wat_path, minimized.wat()).unwrap_or_else(|error| {
        panic!("failed to write minimized memory capture {wat_path:?}: {error}")
    });

    let manifest_line = manifest_line(&id, minimized.expected());
    let manifest_path = directory.join(format!("{id}.manifest.tsv"));
    fs::write(&manifest_path, format!("{manifest_line}\n")).unwrap_or_else(|error| {
        panic!("failed to write memory-capture manifest row {manifest_path:?}: {error}")
    });

    let metadata_path = directory.join(format!("{id}.txt"));
    let metadata = format!(
        "seed=0x{seed:016x}\ncase={case_index}\noriginal={original:?}\nminimized={minimized:?}\nmini={mini:?}\nreference={reference:?}\nmanifest={manifest_line}\n"
    );
    fs::write(&metadata_path, metadata).unwrap_or_else(|error| {
        panic!("failed to write memory-capture metadata {metadata_path:?}: {error}")
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
fn memory_shrinker_reduces_a_boundary_reproducer_monotonically() {
    let original = MemoryCase {
        address: 70_000,
        offset: 0,
        value: 123_456_789,
    };
    let minimized = shrink_case(original, |case| case.address >= 65_533 && case.value != 0);
    assert_eq!(
        minimized,
        MemoryCase {
            address: 65_533,
            offset: 0,
            value: 1,
        }
    );
    assert!(case_rank(minimized) < case_rank(original));
}

#[test]
fn memory_capture_renderer_emits_regression_manifest_compatible_payloads() {
    let id = capture_id(0x0123_4567_89ab_cdef, 9);
    assert_eq!(id, "auto-memory-0123456789abcdef-009");
    assert_eq!(
        manifest_line(&id, MemoryOutcome::I32(42)),
        "auto-memory-0123456789abcdef-009\tauto-memory-0123456789abcdef-009.wat\ti32\t42"
    );
    assert_eq!(
        manifest_line(&id, MemoryOutcome::MemoryOutOfBounds),
        "auto-memory-0123456789abcdef-009\tauto-memory-0123456789abcdef-009.wat\ttrap\tmemory_out_of_bounds"
    );
    assert_eq!(
        MemoryCase {
            address: 16,
            offset: 4,
            value: 42,
        }
        .wat(),
        "(module\n  (memory 1)\n  (func (export \"run\") (result i32)\n    i32.const 16\n    i32.const 42\n    i32.store offset=4\n    i32.const 16\n    i32.load offset=4))\n"
    );
}

#[test]
fn generated_memory_differentials_capture_and_shrink_real_mismatches() {
    const SEED: u64 = 0xa54f_f53a_5f1d_36f1;
    const CASES: usize = 96;
    const OFFSETS: [u32; 9] = [0, 1, 2, 3, 4, 7, 16, 64, 255];

    let engine = Engine::default();
    let mut rng = XorShift64::new(SEED);
    let mut in_bounds = 0_usize;
    let mut out_of_bounds = 0_usize;

    for case_index in 0..CASES {
        let address = if case_index % 4 == 0 {
            65_530 + (rng.next_u64() % 12) as u32
        } else {
            (rng.next_u64() % 65_536) as u32
        };
        let offset = OFFSETS[(rng.next_u64() as usize) % OFFSETS.len()];
        let case = MemoryCase {
            address,
            offset,
            value: rng.next_i32(),
        };
        let expected = case.expected();
        match expected {
            MemoryOutcome::I32(_) => in_bounds += 1,
            MemoryOutcome::MemoryOutOfBounds => out_of_bounds += 1,
        }

        let (mini, reference) = observe(&engine, case);
        if mini != reference {
            assert_eq!(
                reference, expected,
                "reference/model disagreement at seed={SEED:#018x} case={case_index}; refusing to capture a mini-runtime memory regression against an untrusted oracle"
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
                "memory differential mismatch at seed={SEED:#018x} case={case_index}: original={case:?}, minimized={minimized:?}, artifacts={:?}, manifest_row={:?}",
                capture.directory, capture.manifest_line
            );
        }

        assert_eq!(
            mini, expected,
            "mini/model memory mismatch at seed={SEED:#018x} case={case_index}: {case:?}"
        );
        assert_eq!(
            reference, expected,
            "Wasmtime/model memory mismatch at seed={SEED:#018x} case={case_index}: {case:?}"
        );
    }

    assert!(
        in_bounds > 0,
        "memory corpus must exercise successful accesses"
    );
    assert!(
        out_of_bounds > 0,
        "memory corpus must exercise out-of-bounds traps"
    );
}
