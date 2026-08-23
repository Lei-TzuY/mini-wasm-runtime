use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use wasm_parser::{parse_module, ValueType};
use wasm_runtime::{HostCapabilities, HostRegistry, Instance as MiniInstance, Value};
use wasmtime::{
    Engine, Extern, Func, Instance as ReferenceInstance, Module as ReferenceModule, Store,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct ImportCase {
    initial_state: i64,
    salt: i64,
    inputs: Vec<i64>,
}

impl ImportCase {
    fn wat(&self) -> String {
        format!(
            "(module\n  (import \"env\" \"host\" (func $host (param i64) (result i64)))\n  (func (export \"run\") (param i64) (result i64)\n    local.get 0\n    call $host\n    i64.const {}\n    i64.xor))\n",
            self.salt
        )
    }

    fn expected(&self) -> TraceOutcome {
        let mut state = self.initial_state;
        let mut results = Vec::with_capacity(self.inputs.len());
        for input in &self.inputs {
            state = state.wrapping_add(*input);
            results.push(state ^ self.salt);
        }
        TraceOutcome {
            results,
            final_state: state,
            failure_at: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TraceOutcome {
    results: Vec<i64>,
    final_state: i64,
    failure_at: Option<usize>,
}

fn run_mini(bytes: &[u8], case: &ImportCase) -> TraceOutcome {
    let state = Arc::new(Mutex::new(case.initial_state));
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
                    .expect("mini import-capture host-state mutex poisoned");
                *value = value.wrapping_add(args[0].as_i64());
                Ok(Some(Value::I64(*value)))
            },
        )
        .expect("register mini import-capture host function");
    let module = parse_module(bytes).expect("import-capture candidate must parse in mini runtime");
    let mut instance = MiniInstance::with_hosts(module, hosts)
        .expect("import-capture candidate must validate and instantiate in mini runtime");

    let mut results = Vec::with_capacity(case.inputs.len());
    let mut failure_at = None;
    for (call, input) in case.inputs.iter().copied().enumerate() {
        match instance.invoke_export_values("run", &[Value::I64(input)]) {
            Ok(values) => match values.as_slice() {
                [Value::I64(value)] => results.push(*value),
                other => panic!("unexpected mini import-capture result shape: {other:?}"),
            },
            Err(_) => {
                failure_at = Some(call);
                break;
            }
        }
    }
    let final_state = *state.lock().expect("read mini import-capture host state");
    TraceOutcome {
        results,
        final_state,
        failure_at,
    }
}

fn run_reference(engine: &Engine, bytes: &[u8], case: &ImportCase) -> TraceOutcome {
    let module =
        ReferenceModule::new(engine, bytes).expect("import-capture candidate must compile");
    let state = Arc::new(Mutex::new(case.initial_state));
    let callback_state = Arc::clone(&state);
    let mut store = Store::new(engine, ());
    let host = Func::wrap(&mut store, move |input: i64| -> i64 {
        let mut value = callback_state
            .lock()
            .expect("Wasmtime import-capture host-state mutex poisoned");
        *value = value.wrapping_add(input);
        *value
    });
    let instance = ReferenceInstance::new(&mut store, &module, &[Extern::Func(host)])
        .expect("instantiate Wasmtime import-capture candidate");
    let run = instance
        .get_typed_func::<i64, i64>(&mut store, "run")
        .expect("import-capture run export must be [i64] -> [i64]");

    let mut results = Vec::with_capacity(case.inputs.len());
    let mut failure_at = None;
    for (call, input) in case.inputs.iter().copied().enumerate() {
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
        .expect("read Wasmtime import-capture host state");
    TraceOutcome {
        results,
        final_state,
        failure_at,
    }
}

fn observe(engine: &Engine, case: &ImportCase) -> (TraceOutcome, TraceOutcome) {
    let bytes = wat::parse_str(case.wat()).expect("generated import-capture WAT must compile");
    (run_mini(&bytes, case), run_reference(engine, &bytes, case))
}

fn reproduces_reference_backed_mismatch(engine: &Engine, case: &ImportCase) -> bool {
    let (mini, reference) = observe(engine, case);
    mini != reference && reference == case.expected()
}

fn value_rank(value: i64) -> (u64, bool) {
    (value.unsigned_abs(), value.is_negative())
}

fn case_rank(case: &ImportCase) -> (usize, (u64, bool), (u64, bool), Vec<(u64, bool)>) {
    (
        case.inputs.len(),
        value_rank(case.initial_state),
        value_rank(case.salt),
        case.inputs.iter().map(|value| value_rank(*value)).collect(),
    )
}

fn simplification_candidates(value: i64) -> Vec<i64> {
    let original_rank = value_rank(value);
    let mut candidates = Vec::new();

    for candidate in [0_i64, 1, -1, 2, -2] {
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

fn shrink_case(
    mut case: ImportCase,
    mut reproduces: impl FnMut(&ImportCase) -> bool,
) -> ImportCase {
    assert!(
        reproduces(&case),
        "import shrinker requires a reproducing input"
    );

    loop {
        let mut changed = false;

        for new_len in 1..case.inputs.len() {
            let mut candidate = case.clone();
            candidate.inputs.truncate(new_len);
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

        for initial_state in simplification_candidates(case.initial_state) {
            let mut candidate = case.clone();
            candidate.initial_state = initial_state;
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

        for salt in simplification_candidates(case.salt) {
            let mut candidate = case.clone();
            candidate.salt = salt;
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

        'inputs: for index in 0..case.inputs.len() {
            for value in simplification_candidates(case.inputs[index]) {
                let mut candidate = case.clone();
                candidate.inputs[index] = value;
                assert!(case_rank(&candidate) < case_rank(&case));
                if reproduces(&candidate) {
                    case = candidate;
                    changed = true;
                    break 'inputs;
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
    format!("auto-import-host-{seed:016x}-{case_index:03}")
}

fn csv_i64(values: &[i64]) -> String {
    values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn driver_line(id: &str, case: &ImportCase) -> String {
    let expected = case.expected();
    format!(
        "{id}\t{id}.wat\tstateful_i64_add\t{}\t{}\t{}\t{}\t{}",
        case.initial_state,
        case.salt,
        csv_i64(&case.inputs),
        csv_i64(&expected.results),
        expected.final_state
    )
}

fn write_capture(
    seed: u64,
    case_index: usize,
    original: &ImportCase,
    minimized: &ImportCase,
    mini: &TraceOutcome,
    reference: &TraceOutcome,
) -> CaptureFiles {
    let id = capture_id(seed, case_index);
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("differential-captures");
    fs::create_dir_all(&directory).unwrap_or_else(|error| {
        panic!("failed to create import-capture directory {directory:?}: {error}")
    });

    let wat_path = directory.join(format!("{id}.wat"));
    fs::write(&wat_path, minimized.wat()).unwrap_or_else(|error| {
        panic!("failed to write minimized import capture {wat_path:?}: {error}")
    });

    let driver_line = driver_line(&id, minimized);
    let driver_path = directory.join(format!("{id}.import.tsv"));
    fs::write(&driver_path, format!("{driver_line}\n")).unwrap_or_else(|error| {
        panic!("failed to write import-capture driver {driver_path:?}: {error}")
    });

    let metadata_path = directory.join(format!("{id}.txt"));
    let metadata = format!(
        "seed=0x{seed:016x}\ncase={case_index}\noriginal={original:?}\nminimized={minimized:?}\nmini={mini:?}\nreference={reference:?}\ndriver={driver_line}\n"
    );
    fs::write(&metadata_path, metadata).unwrap_or_else(|error| {
        panic!("failed to write import-capture metadata {metadata_path:?}: {error}")
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

    fn next_i64(&mut self) -> i64 {
        self.next_u64() as i64
    }
}

#[test]
fn import_shrinker_reduces_state_salt_inputs_and_sequence_length() {
    let original = ImportCase {
        initial_state: 9_876_543_210,
        salt: -7_654_321,
        inputs: vec![123_456, -234_567, 345_678, -456_789],
    };
    let minimized = shrink_case(original.clone(), |case| {
        case.inputs.len() >= 2
            && case.initial_state != 0
            && case.salt != 0
            && case.inputs[0] != 0
            && case.inputs[1] != 0
    });
    assert_eq!(
        minimized,
        ImportCase {
            initial_state: 1,
            salt: 1,
            inputs: vec![1, 1],
        }
    );
    assert!(case_rank(&minimized) < case_rank(&original));
}

#[test]
fn import_capture_renderer_emits_driver_complete_payload() {
    let id = capture_id(0x0123_4567_89ab_cdef, 5);
    let case = ImportCase {
        initial_state: 10,
        salt: 7,
        inputs: vec![3, -2],
    };
    assert_eq!(id, "auto-import-host-0123456789abcdef-005");
    assert_eq!(
        driver_line(&id, &case),
        "auto-import-host-0123456789abcdef-005\tauto-import-host-0123456789abcdef-005.wat\tstateful_i64_add\t10\t7\t3,-2\t10,12\t11"
    );
}

#[test]
fn generated_import_differentials_capture_and_shrink_real_mismatches() {
    const SEED: u64 = 0x510e_527f_ade6_82d1;
    const CASES: usize = 48;

    let engine = Engine::default();
    let mut rng = XorShift64::new(SEED);

    for case_index in 0..CASES {
        let calls = 1 + (rng.next_u64() % 5) as usize;
        let initial_state = match case_index % 8 {
            0 => i64::MAX,
            1 => i64::MIN,
            _ => rng.next_i64(),
        };
        let salt = match case_index % 8 {
            2 => -1,
            3 => 0,
            _ => rng.next_i64(),
        };
        let mut inputs = Vec::with_capacity(calls);
        for call in 0..calls {
            let input = match (case_index + call) % 11 {
                0 => i64::MAX,
                1 => i64::MIN,
                2 => -1,
                3 => 0,
                _ => rng.next_i64(),
            };
            inputs.push(input);
        }
        let case = ImportCase {
            initial_state,
            salt,
            inputs,
        };
        let expected = case.expected();
        let (mini, reference) = observe(&engine, &case);

        if mini != reference {
            assert_eq!(
                reference, expected,
                "reference/model disagreement at seed={SEED:#018x} case={case_index}; refusing to capture a mini-runtime import regression against an untrusted oracle"
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
                "import differential mismatch at seed={SEED:#018x} case={case_index}: original={case:?}, minimized={minimized:?}, artifacts={:?}, driver={:?}",
                capture.directory, capture.driver_line
            );
        }

        assert_eq!(
            mini, expected,
            "mini/model import mismatch at seed={SEED:#018x} case={case_index}: {case:?}"
        );
        assert_eq!(
            reference, expected,
            "Wasmtime/model import mismatch at seed={SEED:#018x} case={case_index}: {case:?}"
        );
    }
}
