use std::panic::{catch_unwind, AssertUnwindSafe};

use wasm_parser::parse_module;
use wasm_validator::validate;

const I32: u8 = 0x7f;
const SEED: u64 = 0x243f_6a88_85a3_08d3;

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
}

fn push_u32(bytes: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        bytes.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn push_name(bytes: &mut Vec<u8>, name: &str) {
    push_u32(bytes, name.len() as u32);
    bytes.extend_from_slice(name.as_bytes());
}

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn header() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

fn arithmetic_seed() -> Vec<u8> {
    let mut module = header();
    push_section(&mut module, 1, &[0x01, 0x60, 0x02, I32, I32, 0x01, I32]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);

    let body = [0x00, 0x20, 0x00, 0x20, 0x01, 0x6a, 0x0b];
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn memory_data_seed() -> Vec<u8> {
    let mut module = header();
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x01, I32]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 5, &[0x01, 0x00, 0x01]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);

    let body = [0x00, 0x41, 0x00, 0x28, 0x02, 0x00, 0x0b];
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);

    push_section(
        &mut module,
        11,
        &[0x01, 0x00, 0x41, 0x00, 0x0b, 0x04, 0x78, 0x56, 0x34, 0x12],
    );
    module
}

fn table_global_seed() -> Vec<u8> {
    let mut module = header();
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x01, I32]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 4, &[0x01, 0x70, 0x00, 0x01]);
    push_section(&mut module, 6, &[0x01, I32, 0x00, 0x41, 0x07, 0x0b]);

    let mut exports = vec![0x03];
    push_name(&mut exports, "run");
    exports.extend_from_slice(&[0x00, 0x00]);
    push_name(&mut exports, "tab");
    exports.extend_from_slice(&[0x01, 0x00]);
    push_name(&mut exports, "g");
    exports.extend_from_slice(&[0x03, 0x00]);
    push_section(&mut module, 7, &exports);

    push_section(&mut module, 9, &[0x01, 0x00, 0x41, 0x00, 0x0b, 0x01, 0x00]);

    let body = [0x00, 0x23, 0x00, 0x0b];
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn exercise_pipeline(bytes: &[u8], label: &str) -> (bool, bool) {
    let parsed = catch_unwind(|| parse_module(bytes));
    let parsed = match parsed {
        Ok(result) => result,
        Err(_) => panic!("parser panicked for deterministic mutation: {label}"),
    };

    let module = match parsed {
        Ok(module) => module,
        Err(_) => return (false, false),
    };

    let validated = catch_unwind(AssertUnwindSafe(|| validate(&module)));
    match validated {
        Ok(Ok(())) => (true, true),
        Ok(Err(_)) => (true, false),
        Err(_) => panic!("validator panicked for parsed deterministic mutation: {label}"),
    }
}

#[test]
fn deterministic_binary_mutations_never_panic_parser_or_validator() {
    let seeds = [
        ("arithmetic", arithmetic_seed()),
        ("memory-data", memory_data_seed()),
        ("table-global", table_global_seed()),
    ];

    let mut total = 0usize;
    let mut parsed = 0usize;
    let mut validated = 0usize;

    for (seed_index, (seed_name, seed)) in seeds.into_iter().enumerate() {
        let baseline = exercise_pipeline(&seed, &format!("{seed_name}:baseline"));
        assert_eq!(
            baseline,
            (true, true),
            "seed module must be valid: {seed_name}"
        );
        total += 1;
        parsed += 1;
        validated += 1;

        for cut in 0..seed.len() {
            let (did_parse, did_validate) =
                exercise_pipeline(&seed[..cut], &format!("{seed_name}:truncate:{cut}"));
            total += 1;
            parsed += usize::from(did_parse);
            validated += usize::from(did_validate);
        }

        for index in 0..seed.len() {
            for mask in [0x01u8, 0x40, 0x80, 0xff] {
                let mut mutated = seed.clone();
                mutated[index] ^= mask;
                let (did_parse, did_validate) = exercise_pipeline(
                    &mutated,
                    &format!("{seed_name}:xor:index={index}:mask={mask:#04x}"),
                );
                total += 1;
                parsed += usize::from(did_parse);
                validated += usize::from(did_validate);
            }
        }

        let mut rng = XorShift64::new(SEED ^ ((seed_index as u64 + 1) * 0x9e37_79b9));
        for case in 0..256 {
            let mut mutated = seed.clone();
            let edits = (rng.next_u64() % 4 + 1) as usize;
            for _ in 0..edits {
                let index = (rng.next_u64() as usize) % mutated.len();
                let mask = (rng.next_u64() as u8) | 0x01;
                mutated[index] ^= mask;
            }
            let (did_parse, did_validate) = exercise_pipeline(
                &mutated,
                &format!("{seed_name}:generated:{case}:edits={edits}"),
            );
            total += 1;
            parsed += usize::from(did_parse);
            validated += usize::from(did_validate);
        }
    }

    assert!(
        total >= 1_000,
        "mutation corpus unexpectedly became too small"
    );
    assert!(parsed >= 3, "valid baseline seeds must reach the validator");
    assert!(validated >= 3, "valid baseline seeds must remain valid");
}
