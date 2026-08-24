use std::{fs, path::PathBuf};

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
struct MemoryCase {
    address: u32,
    initial_value: i32,
    override_call: usize,
    override_value: i32,
    inputs: Vec<i32>,
}

impl MemoryCase {
    fn wat(&self) -> String {
        format!(
            "(module\n  (import \"env\" \"mem\" (memory 1 2))\n  (func (export \"run\") (param i32) (result i32)\n    i32.const {}\n    i32.const {}\n    i32.load\n    local.get 0\n    i32.add\n    i32.store\n    i32.const {}\n    i32.load))\n",
            self.address as i32, self.address as i32, self.address as i32
        )
    }

    fn expected(&self) -> TraceOutcome {
        assert!(self.address <= LAST_VALID_I32_ADDRESS);
        assert!(self.override_call < self.inputs.len());
        let mut value = self.initial_value;
        let mut results = Vec::with_capacity(self.inputs.len());
        for (call, input) in self.inputs.iter().copied().enumerate() {
            if call == self.override_call {
                value = self.override_value;
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TraceOutcome {
    results: Vec<i32>,
    final_value: i32,
    failure_at: Option<usize>,
}

fn read_mini_i32(memory: &MemoryHandle, address: u32) -> i32 {
    let bytes = memory
        .read(address, WIDTH as usize)
        .expect("read imported-memory backing");
    i32::from_le_bytes(bytes.try_into().expect("four-byte imported-memory read"))
}

fn run_mini(bytes: &[u8], case: &MemoryCase) -> TraceOutcome {
    let memory = MemoryHandle::new(1, Some(2)).expect("create mini imported memory");
    memory
        .write(case.address, &case.initial_value.to_le_bytes())
        .expect("seed mini imported memory");
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "mem", memory.clone())
        .expect("register mini imported memory");
    let module = parse_module(bytes).expect("imported-memory capture candidate must parse");
    let mut instance = MiniInstance::with_hosts(module, hosts)
        .expect("imported-memory capture candidate must instantiate");

    let mut results = Vec::with_capacity(case.inputs.len());
    let mut failure_at = None;
    for (call, input) in case.inputs.iter().copied().enumerate() {
        if call == case.override_call {
            memory
                .write(case.address, &case.override_value.to_le_bytes())
                .expect("override mini imported memory");
        }
        match instance.invoke_export_values("run", &[Value::I32(input)]) {
            Ok(values) => match values.as_slice() {
                [Value::I32(value)] => results.push(*value),
                other => panic!("unexpected mini imported-memory result shape: {other:?}"),
            },
            Err(_) => {
                failure_at = Some(call);
                break;
            }
        }
    }

    TraceOutcome {
        results,
        final_value: read_mini_i32(&memory, case.address),
        failure_at,
    }
}

fn read_reference_i32(memory: Memory, store: &Store<()>, address: u32) -> i32 {
    let mut bytes = [0_u8; WIDTH as usize];
    memory
        .read(store, address as usize, &mut bytes)
        .expect("read Wasmtime imported-memory backing");
    i32::from_le_bytes(bytes)
}

fn run_reference(engine: &Engine, bytes: &[u8], case: &MemoryCase) -> TraceOutcome {
    let module = ReferenceModule::new(engine, bytes)
        .expect("imported-memory capture candidate must compile in Wasmtime");
    let mut store = Store::new(engine, ());
    let memory = Memory::new(&mut store, MemoryType::new(1, Some(2)))
        .expect("create Wasmtime imported memory");
    memory
        .write(
            &mut store,
            case.address as usize,
            &case.initial_value.to_le_bytes(),
        )
        .expect("seed Wasmtime imported memory");
    let instance = ReferenceInstance::new(&mut store, &module, &[Extern::Memory(memory)])
        .expect("instantiate Wasmtime imported-memory capture candidate");
    let run = instance
        .get_typed_func::<i32, i32>(&mut store, "run")
        .expect("imported-memory run export must be [i32] -> [i32]");

    let mut results = Vec::with_capacity(case.inputs.len());
    let mut failure_at = None;
    for (call, input) in case.inputs.iter().copied().enumerate() {
        if call == case.override_call {
            memory
                .write(
                    &mut store,
                    case.address as usize,
                    &case.override_value.to_le_bytes(),
                )
                .expect("override Wasmtime imported memory");
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
        final_value: read_reference_i32(memory, &store, case.address),
        failure_at,
    }
}

fn observe(engine: &Engine, case: &MemoryCase) -> (TraceOutcome, TraceOutcome) {
    let bytes = wat::parse_str(case.wat()).expect("generated imported-memory WAT must compile");
    (run_mini(&bytes, case), run_reference(engine, &bytes, case))
}

fn reproduces_reference_backed_mismatch(engine: &Engine, case: &MemoryCase) -> bool {
    let (mini, reference) = observe(engine, case);
    mini != reference && reference == case.expected()
}

type ValueRank = (u64, bool);
type CaseRank = (usize, usize, u32, ValueRank, ValueRank, Vec<ValueRank>);

fn value_rank(value: i32) -> ValueRank {
    (i64::from(value).unsigned_abs(), value.is_negative())
}

fn case_rank(case: &MemoryCase) -> CaseRank {
    (
        case.inputs.len(),
        case.override_call,
        case.address,
        value_rank(case.initial_value),
        value_rank(case.override_value),
        case.inputs.iter().map(|value| value_rank(*value)).collect(),
    )
}

fn address_candidates(value: u32) -> Vec<u32> {
    let mut candidates = Vec::new();
    for candidate in [0, 1, 2, 3, 4, 16, 64, 255, 1024, LAST_VALID_I32_ADDRESS] {
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

fn shrink_case(
    mut case: MemoryCase,
    mut reproduces: impl FnMut(&MemoryCase) -> bool,
) -> MemoryCase {
    assert!(
        reproduces(&case),
        "imported-memory shrinker requires a reproducing input"
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

        for address in address_candidates(case.address) {
            let mut candidate = case.clone();
            candidate.address = address;
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

        for initial_value in value_candidates(case.initial_value) {
            let mut candidate = case.clone();
            candidate.initial_value = initial_value;
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

        for override_value in value_candidates(case.override_value) {
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
            for value in value_candidates(case.inputs[index]) {
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
    format!("auto-import-memory-{seed:016x}-{case_index:03}")
}

fn csv_i32(values: &[i32]) -> String {
    values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn driver_line(id: &str, case: &MemoryCase) -> String {
    let expected = case.expected();
    format!(
        "{id}\t{id}.wat\tmutable_i32_memory\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        case.address,
        case.initial_value,
        case.override_call,
        case.override_value,
        csv_i32(&case.inputs),
        csv_i32(&expected.results),
        expected.final_value
    )
}

fn write_capture(
    seed: u64,
    case_index: usize,
    original: &MemoryCase,
    minimized: &MemoryCase,
    mini: &TraceOutcome,
    reference: &TraceOutcome,
) -> CaptureFiles {
    let id = capture_id(seed, case_index);
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("differential-captures");
    fs::create_dir_all(&directory).unwrap_or_else(|error| {
        panic!("failed to create imported-memory capture directory {directory:?}: {error}")
    });

    let wat_path = directory.join(format!("{id}.wat"));
    fs::write(&wat_path, minimized.wat()).unwrap_or_else(|error| {
        panic!("failed to write minimized imported-memory capture {wat_path:?}: {error}")
    });

    let driver_line = driver_line(&id, minimized);
    let driver_path = directory.join(format!("{id}.memory.tsv"));
    fs::write(&driver_path, format!("{driver_line}\n")).unwrap_or_else(|error| {
        panic!("failed to write imported-memory capture driver {driver_path:?}: {error}")
    });

    let metadata_path = directory.join(format!("{id}.txt"));
    let metadata = format!(
        "seed=0x{seed:016x}\ncase={case_index}\noriginal={original:?}\nminimized={minimized:?}\nmini={mini:?}\nreference={reference:?}\ndriver={driver_line}\n"
    );
    fs::write(&metadata_path, metadata).unwrap_or_else(|error| {
        panic!("failed to write imported-memory capture metadata {metadata_path:?}: {error}")
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
fn imported_memory_shrinker_reduces_address_override_state_and_inputs() {
    let original = MemoryCase {
        address: LAST_VALID_I32_ADDRESS,
        initial_value: 123_456,
        override_call: 2,
        override_value: -987_654,
        inputs: vec![33, -44, 55, -66],
    };
    let minimized = shrink_case(original.clone(), |case| {
        case.inputs.len() >= 2
            && case.initial_value != 0
            && case.override_value != 0
            && case.inputs[0] != 0
            && case.inputs[1] != 0
    });
    assert_eq!(
        minimized,
        MemoryCase {
            address: 0,
            initial_value: 1,
            override_call: 0,
            override_value: 1,
            inputs: vec![1, 1],
        }
    );
    assert!(case_rank(&minimized) < case_rank(&original));
}

#[test]
fn imported_memory_capture_renderer_emits_replay_compatible_driver() {
    let id = capture_id(0x0123_4567_89ab_cdef, 11);
    let case = MemoryCase {
        address: 64,
        initial_value: 10,
        override_call: 1,
        override_value: 100,
        inputs: vec![3, -2, 4],
    };
    assert_eq!(id, "auto-import-memory-0123456789abcdef-011");
    assert_eq!(
        driver_line(&id, &case),
        "auto-import-memory-0123456789abcdef-011\tauto-import-memory-0123456789abcdef-011.wat\tmutable_i32_memory\t64\t10\t1\t100\t3,-2,4\t13,98,102\t102"
    );
}

#[test]
fn generated_imported_memory_differentials_capture_and_shrink_real_mismatches() {
    const SEED: u64 = 0xbb67_ae85_84ca_a73b;
    const CASES: usize = 48;

    let engine = Engine::default();
    let mut rng = XorShift64::new(SEED);

    for case_index in 0..CASES {
        let calls = 2 + (rng.next_u64() % 4) as usize;
        let address = match case_index % 8 {
            0 => LAST_VALID_I32_ADDRESS,
            1 => LAST_VALID_I32_ADDRESS - 1,
            2 => 1,
            3 => 64,
            _ => (rng.next_u64() % u64::from(LAST_VALID_I32_ADDRESS + 1)) as u32,
        };
        let initial_value = match case_index % 8 {
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
        let case = MemoryCase {
            address,
            initial_value,
            override_call,
            override_value,
            inputs,
        };
        let expected = case.expected();
        let (mini, reference) = observe(&engine, &case);

        if mini != reference {
            assert_eq!(
                reference, expected,
                "reference/model disagreement at seed={SEED:#018x} case={case_index}; refusing to capture a mini-runtime imported-memory regression against an untrusted oracle"
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
                "imported-memory differential mismatch at seed={SEED:#018x} case={case_index}: original={case:?}, minimized={minimized:?}, artifacts={:?}, driver={:?}",
                capture.directory, capture.driver_line
            );
        }

        assert_eq!(
            mini, expected,
            "mini/model imported-memory mismatch at seed={SEED:#018x} case={case_index}: {case:?}"
        );
        assert_eq!(
            reference, expected,
            "Wasmtime/model imported-memory mismatch at seed={SEED:#018x} case={case_index}: {case:?}"
        );
    }
}
