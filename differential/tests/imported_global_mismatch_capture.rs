use std::{fs, path::PathBuf};

use wasm_parser::parse_module;
use wasm_runtime::{GlobalHandle, HostRegistry, Instance as MiniInstance, Value};
use wasmtime::{
    Engine, Extern, Global, GlobalType, Instance as ReferenceInstance, Module as ReferenceModule,
    Mutability, Store, Val, ValType,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct GlobalCase {
    initial_state: i32,
    override_call: usize,
    override_value: i32,
    inputs: Vec<i32>,
}

impl GlobalCase {
    fn wat(&self) -> &'static str {
        "(module\n  (import \"env\" \"g\" (global $g (mut i32)))\n  (func (export \"run\") (param i32) (result i32)\n    global.get $g\n    local.get 0\n    i32.add\n    global.set $g\n    global.get $g))\n"
    }

    fn expected(&self) -> TraceOutcome {
        assert!(self.override_call < self.inputs.len());
        let mut state = self.initial_state;
        let mut results = Vec::with_capacity(self.inputs.len());
        for (call, input) in self.inputs.iter().copied().enumerate() {
            if call == self.override_call {
                state = self.override_value;
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TraceOutcome {
    results: Vec<i32>,
    final_state: i32,
    failure_at: Option<usize>,
}

fn run_mini(bytes: &[u8], case: &GlobalCase) -> TraceOutcome {
    let global = GlobalHandle::mutable(Value::I32(case.initial_state));
    let mut hosts = HostRegistry::new();
    hosts
        .register_global("env", "g", global.clone())
        .expect("register mini imported global");
    let module = parse_module(bytes).expect("imported-global capture candidate must parse");
    let mut instance = MiniInstance::with_hosts(module, hosts)
        .expect("imported-global capture candidate must instantiate");

    let mut results = Vec::with_capacity(case.inputs.len());
    let mut failure_at = None;
    for (call, input) in case.inputs.iter().copied().enumerate() {
        if call == case.override_call {
            global
                .set(Value::I32(case.override_value))
                .expect("override mini imported global");
        }
        match instance.invoke_export_values("run", &[Value::I32(input)]) {
            Ok(values) => match values.as_slice() {
                [Value::I32(value)] => results.push(*value),
                other => panic!("unexpected mini imported-global result shape: {other:?}"),
            },
            Err(_) => {
                failure_at = Some(call);
                break;
            }
        }
    }
    let final_state = match global.get() {
        Value::I32(value) => value,
        other => panic!("unexpected mini imported-global backing type: {other:?}"),
    };
    TraceOutcome {
        results,
        final_state,
        failure_at,
    }
}

fn run_reference(engine: &Engine, bytes: &[u8], case: &GlobalCase) -> TraceOutcome {
    let module = ReferenceModule::new(engine, bytes)
        .expect("imported-global capture candidate must compile in Wasmtime");
    let mut store = Store::new(engine, ());
    let global = Global::new(
        &mut store,
        GlobalType::new(ValType::I32, Mutability::Var),
        Val::I32(case.initial_state),
    )
    .expect("create Wasmtime imported global");
    let instance = ReferenceInstance::new(&mut store, &module, &[Extern::Global(global)])
        .expect("instantiate Wasmtime imported-global capture candidate");
    let run = instance
        .get_typed_func::<i32, i32>(&mut store, "run")
        .expect("imported-global run export must be [i32] -> [i32]");

    let mut results = Vec::with_capacity(case.inputs.len());
    let mut failure_at = None;
    for (call, input) in case.inputs.iter().copied().enumerate() {
        if call == case.override_call {
            global
                .set(&mut store, Val::I32(case.override_value))
                .expect("override Wasmtime imported global");
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
        other => panic!("unexpected Wasmtime imported-global backing type: {other:?}"),
    };
    TraceOutcome {
        results,
        final_state,
        failure_at,
    }
}

fn observe(engine: &Engine, case: &GlobalCase) -> (TraceOutcome, TraceOutcome) {
    let bytes = wat::parse_str(case.wat()).expect("generated imported-global WAT must compile");
    (run_mini(&bytes, case), run_reference(engine, &bytes, case))
}

fn reproduces_reference_backed_mismatch(engine: &Engine, case: &GlobalCase) -> bool {
    let (mini, reference) = observe(engine, case);
    mini != reference && reference == case.expected()
}

type ValueRank = (u64, bool);
type CaseRank = (usize, usize, ValueRank, ValueRank, Vec<ValueRank>);

fn value_rank(value: i32) -> ValueRank {
    (i64::from(value).unsigned_abs(), value.is_negative())
}

fn case_rank(case: &GlobalCase) -> CaseRank {
    (
        case.inputs.len(),
        case.override_call,
        value_rank(case.initial_state),
        value_rank(case.override_value),
        case.inputs.iter().map(|value| value_rank(*value)).collect(),
    )
}

fn simplification_candidates(value: i32) -> Vec<i32> {
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

fn shrink_case(
    mut case: GlobalCase,
    mut reproduces: impl FnMut(&GlobalCase) -> bool,
) -> GlobalCase {
    assert!(
        reproduces(&case),
        "imported-global shrinker requires a reproducing input"
    );

    loop {
        let mut changed = false;

        for new_len in (case.override_call + 1)..case.inputs.len() {
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

        for override_call in 0..case.override_call {
            let mut candidate = case.clone();
            candidate.override_call = override_call;
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

        for override_value in simplification_candidates(case.override_value) {
            let mut candidate = case.clone();
            candidate.override_value = override_value;
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
    format!("auto-import-global-{seed:016x}-{case_index:03}")
}

fn csv_i32(values: &[i32]) -> String {
    values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn driver_line(id: &str, case: &GlobalCase) -> String {
    let expected = case.expected();
    format!(
        "{id}\t{id}.wat\tmutable_i32_global\t{}\t{}\t{}\t{}\t{}\t{}",
        case.initial_state,
        case.override_call,
        case.override_value,
        csv_i32(&case.inputs),
        csv_i32(&expected.results),
        expected.final_state
    )
}

fn write_capture(
    seed: u64,
    case_index: usize,
    original: &GlobalCase,
    minimized: &GlobalCase,
    mini: &TraceOutcome,
    reference: &TraceOutcome,
) -> CaptureFiles {
    let id = capture_id(seed, case_index);
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("differential-captures");
    fs::create_dir_all(&directory).unwrap_or_else(|error| {
        panic!("failed to create imported-global capture directory {directory:?}: {error}")
    });

    let wat_path = directory.join(format!("{id}.wat"));
    fs::write(&wat_path, minimized.wat()).unwrap_or_else(|error| {
        panic!("failed to write minimized imported-global capture {wat_path:?}: {error}")
    });

    let driver_line = driver_line(&id, minimized);
    let driver_path = directory.join(format!("{id}.global.tsv"));
    fs::write(&driver_path, format!("{driver_line}\n")).unwrap_or_else(|error| {
        panic!("failed to write imported-global capture driver {driver_path:?}: {error}")
    });

    let metadata_path = directory.join(format!("{id}.txt"));
    let metadata = format!(
        "seed=0x{seed:016x}\ncase={case_index}\noriginal={original:?}\nminimized={minimized:?}\nmini={mini:?}\nreference={reference:?}\ndriver={driver_line}\n"
    );
    fs::write(&metadata_path, metadata).unwrap_or_else(|error| {
        panic!("failed to write imported-global capture metadata {metadata_path:?}: {error}")
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
fn imported_global_shrinker_reduces_override_state_and_inputs() {
    let original = GlobalCase {
        initial_state: 123_456,
        override_call: 2,
        override_value: -987_654,
        inputs: vec![33, -44, 55, -66],
    };
    let minimized = shrink_case(original.clone(), |case| {
        case.inputs.len() >= 2
            && case.override_call <= 1
            && case.initial_state != 0
            && case.override_value != 0
            && case.inputs[0] != 0
            && case.inputs[1] != 0
    });
    assert_eq!(
        minimized,
        GlobalCase {
            initial_state: 1,
            override_call: 0,
            override_value: 1,
            inputs: vec![1, 1],
        }
    );
    assert!(case_rank(&minimized) < case_rank(&original));
}

#[test]
fn imported_global_capture_renderer_emits_replay_compatible_driver() {
    let id = capture_id(0x0123_4567_89ab_cdef, 7);
    let case = GlobalCase {
        initial_state: 10,
        override_call: 1,
        override_value: 100,
        inputs: vec![3, -2, 4],
    };
    assert_eq!(id, "auto-import-global-0123456789abcdef-007");
    assert_eq!(
        driver_line(&id, &case),
        "auto-import-global-0123456789abcdef-007\tauto-import-global-0123456789abcdef-007.wat\tmutable_i32_global\t10\t1\t100\t3,-2,4\t13,98,102\t102"
    );
}

#[test]
fn generated_imported_global_differentials_capture_and_shrink_real_mismatches() {
    const SEED: u64 = 0x6a09_e667_f3bc_c909;
    const CASES: usize = 48;

    let engine = Engine::default();
    let mut rng = XorShift64::new(SEED);

    for case_index in 0..CASES {
        let calls = 2 + (rng.next_u64() % 4) as usize;
        let initial_state = match case_index % 8 {
            0 => i32::MAX,
            1 => i32::MIN,
            _ => rng.next_i32(),
        };
        let override_call = (rng.next_u64() as usize) % calls;
        let override_value = match case_index % 8 {
            2 => i32::MAX,
            3 => i32::MIN,
            4 => 0,
            _ => rng.next_i32(),
        };
        let mut inputs = Vec::with_capacity(calls);
        for call in 0..calls {
            let input = match (case_index + call) % 11 {
                0 => i32::MAX,
                1 => i32::MIN,
                2 => -1,
                3 => 0,
                _ => rng.next_i32(),
            };
            inputs.push(input);
        }
        let case = GlobalCase {
            initial_state,
            override_call,
            override_value,
            inputs,
        };
        let expected = case.expected();
        let (mini, reference) = observe(&engine, &case);

        if mini != reference {
            assert_eq!(
                reference, expected,
                "reference/model disagreement at seed={SEED:#018x} case={case_index}; refusing to capture a mini-runtime imported-global regression against an untrusted oracle"
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
                "imported-global differential mismatch at seed={SEED:#018x} case={case_index}: original={case:?}, minimized={minimized:?}, artifacts={:?}, driver={:?}",
                capture.directory, capture.driver_line
            );
        }

        assert_eq!(
            mini, expected,
            "mini/model imported-global mismatch at seed={SEED:#018x} case={case_index}: {case:?}"
        );
        assert_eq!(
            reference, expected,
            "Wasmtime/model imported-global mismatch at seed={SEED:#018x} case={case_index}: {case:?}"
        );
    }
}
