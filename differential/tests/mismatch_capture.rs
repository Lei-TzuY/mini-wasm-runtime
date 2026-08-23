use std::{fs, path::PathBuf};

use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Add,
    Sub,
    Mul,
    And,
    Or,
    Xor,
    Shl,
    ShrS,
    ShrU,
    Rotl,
    Rotr,
}

impl Op {
    fn from_selector(selector: u64) -> Self {
        match selector % 11 {
            0 => Self::Add,
            1 => Self::Sub,
            2 => Self::Mul,
            3 => Self::And,
            4 => Self::Or,
            5 => Self::Xor,
            6 => Self::Shl,
            7 => Self::ShrS,
            8 => Self::ShrU,
            9 => Self::Rotl,
            _ => Self::Rotr,
        }
    }

    fn wat(self) -> &'static str {
        match self {
            Self::Add => "i32.add",
            Self::Sub => "i32.sub",
            Self::Mul => "i32.mul",
            Self::And => "i32.and",
            Self::Or => "i32.or",
            Self::Xor => "i32.xor",
            Self::Shl => "i32.shl",
            Self::ShrS => "i32.shr_s",
            Self::ShrU => "i32.shr_u",
            Self::Rotl => "i32.rotl",
            Self::Rotr => "i32.rotr",
        }
    }

    fn apply(self, lhs: i32, rhs: i32) -> i32 {
        let shift = (rhs as u32) & 31;
        match self {
            Self::Add => lhs.wrapping_add(rhs),
            Self::Sub => lhs.wrapping_sub(rhs),
            Self::Mul => lhs.wrapping_mul(rhs),
            Self::And => lhs & rhs,
            Self::Or => lhs | rhs,
            Self::Xor => lhs ^ rhs,
            Self::Shl => lhs.wrapping_shl(shift),
            Self::ShrS => lhs >> shift,
            Self::ShrU => ((lhs as u32) >> shift) as i32,
            Self::Rotl => (lhs as u32).rotate_left(shift) as i32,
            Self::Rotr => (lhs as u32).rotate_right(shift) as i32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct I32Case {
    lhs: i32,
    rhs: i32,
    op: Op,
}

impl I32Case {
    fn expected(self) -> i32 {
        self.op.apply(self.lhs, self.rhs)
    }

    fn wat(self) -> String {
        format!(
            "(module\n  (func (export \"run\") (result i32)\n    i32.const {}\n    i32.const {}\n    {}))\n",
            self.lhs,
            self.rhs,
            self.op.wat()
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EngineOutcome {
    I32(i32),
    ExecutionFailure,
}

fn run_mini(bytes: &[u8]) -> EngineOutcome {
    let module = parse_module(bytes).expect("capture candidate must parse in mini runtime");
    let mut instance = MiniInstance::new(module)
        .expect("capture candidate must validate and instantiate in mini runtime");
    match instance.invoke_export_values("run", &[]) {
        Ok(values) => match values.as_slice() {
            [Value::I32(value)] => EngineOutcome::I32(*value),
            other => panic!("unexpected mini capture result shape: {other:?}"),
        },
        Err(_) => EngineOutcome::ExecutionFailure,
    }
}

fn run_reference(engine: &Engine, bytes: &[u8]) -> EngineOutcome {
    let module = ReferenceModule::new(engine, bytes).expect("capture candidate must compile");
    let mut store = Store::new(engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("capture candidate must instantiate in Wasmtime");
    let run = instance
        .get_typed_func::<(), i32>(&mut store, "run")
        .expect("capture run export must be [] -> [i32]");
    match run.call(&mut store, ()) {
        Ok(value) => EngineOutcome::I32(value),
        Err(_) => EngineOutcome::ExecutionFailure,
    }
}

fn observe(engine: &Engine, case: I32Case) -> (EngineOutcome, EngineOutcome) {
    let bytes = wat::parse_str(case.wat()).expect("generated capture WAT must compile");
    (run_mini(&bytes), run_reference(engine, &bytes))
}

fn reproduces_reference_backed_mismatch(engine: &Engine, case: I32Case) -> bool {
    let (mini, reference) = observe(engine, case);
    mini != reference && reference == EngineOutcome::I32(case.expected())
}

fn value_rank(value: i32) -> (u64, bool) {
    (i64::from(value).unsigned_abs(), value.is_negative())
}

fn simplification_candidates(value: i32) -> Vec<i32> {
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

fn shrink_case(mut case: I32Case, mut reproduces: impl FnMut(I32Case) -> bool) -> I32Case {
    assert!(reproduces(case), "shrinker requires a reproducing input");

    loop {
        let mut changed = false;

        for lhs in simplification_candidates(case.lhs) {
            let candidate = I32Case { lhs, ..case };
            if reproduces(candidate) {
                case = candidate;
                changed = true;
                break;
            }
        }
        if changed {
            continue;
        }

        for rhs in simplification_candidates(case.rhs) {
            let candidate = I32Case { rhs, ..case };
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
    format!("auto-i32-{seed:016x}-{case_index:03}")
}

fn manifest_line(id: &str, expected: i32) -> String {
    format!("{id}\t{id}.wat\ti32\t{expected}")
}

fn write_capture(
    seed: u64,
    case_index: usize,
    original: I32Case,
    minimized: I32Case,
    mini: EngineOutcome,
    reference: EngineOutcome,
) -> CaptureFiles {
    let id = capture_id(seed, case_index);
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("differential-captures");
    fs::create_dir_all(&directory).unwrap_or_else(|error| {
        panic!("failed to create capture directory {directory:?}: {error}")
    });

    let wat_path = directory.join(format!("{id}.wat"));
    fs::write(&wat_path, minimized.wat())
        .unwrap_or_else(|error| panic!("failed to write minimized capture {wat_path:?}: {error}"));

    let manifest_line = manifest_line(&id, minimized.expected());
    let manifest_path = directory.join(format!("{id}.manifest.tsv"));
    fs::write(&manifest_path, format!("{manifest_line}\n")).unwrap_or_else(|error| {
        panic!("failed to write capture manifest row {manifest_path:?}: {error}")
    });

    let metadata_path = directory.join(format!("{id}.txt"));
    let metadata = format!(
        "seed=0x{seed:016x}\ncase={case_index}\noriginal={original:?}\nminimized={minimized:?}\nmini={mini:?}\nreference={reference:?}\nmanifest={manifest_line}\n"
    );
    fs::write(&metadata_path, metadata).unwrap_or_else(|error| {
        panic!("failed to write capture metadata {metadata_path:?}: {error}")
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
fn shrinker_reduces_a_synthetic_reproducer_monotonically() {
    let original = I32Case {
        lhs: 987_654_321,
        rhs: -123_456_789,
        op: Op::Xor,
    };
    let minimized = shrink_case(original, |case| case.lhs != 0 && case.rhs != 0);
    assert_eq!(
        minimized,
        I32Case {
            lhs: 1,
            rhs: 1,
            op: Op::Xor,
        }
    );
    assert!(value_rank(minimized.lhs) < value_rank(original.lhs));
    assert!(value_rank(minimized.rhs) < value_rank(original.rhs));
}

#[test]
fn capture_renderer_emits_regression_manifest_compatible_payload() {
    let id = capture_id(0x0123_4567_89ab_cdef, 7);
    assert_eq!(id, "auto-i32-0123456789abcdef-007");
    assert_eq!(
        manifest_line(&id, 42),
        "auto-i32-0123456789abcdef-007\tauto-i32-0123456789abcdef-007.wat\ti32\t42"
    );
    assert_eq!(
        I32Case {
            lhs: 40,
            rhs: 2,
            op: Op::Add,
        }
        .wat(),
        "(module\n  (func (export \"run\") (result i32)\n    i32.const 40\n    i32.const 2\n    i32.add))\n"
    );
}

#[test]
fn generated_differentials_capture_and_shrink_real_mismatches() {
    const SEED: u64 = 0x3c6e_f372_fe94_f82b;
    const CASES: usize = 64;

    let engine = Engine::default();
    let mut rng = XorShift64::new(SEED);

    for case_index in 0..CASES {
        let case = I32Case {
            lhs: rng.next_i32(),
            rhs: rng.next_i32(),
            op: Op::from_selector(rng.next_u64()),
        };
        let expected = EngineOutcome::I32(case.expected());
        let (mini, reference) = observe(&engine, case);

        if mini != reference {
            assert_eq!(
                reference, expected,
                "reference/model disagreement at seed={SEED:#018x} case={case_index}; refusing to capture a mini-runtime regression against an untrusted oracle"
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
                "differential mismatch at seed={SEED:#018x} case={case_index}: original={case:?}, minimized={minimized:?}, artifacts={:?}, manifest_row={:?}",
                capture.directory, capture.manifest_line
            );
        }

        assert_eq!(
            mini, expected,
            "mini/model mismatch at seed={SEED:#018x} case={case_index}: {case:?}"
        );
        assert_eq!(
            reference, expected,
            "Wasmtime/model mismatch at seed={SEED:#018x} case={case_index}: {case:?}"
        );
    }
}
