use std::{collections::HashSet, fs, path::PathBuf};

use wasm_parser::parse_module;
use wasm_validator::validate;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expectation {
    Valid,
    ValidationError,
    ParseError,
}

impl Expectation {
    fn parse(raw: &str) -> Self {
        match raw {
            "valid" => Self::Valid,
            "validation-error" => Self::ValidationError,
            "parse-error" => Self::ParseError,
            other => panic!("unknown fuzz-seed expectation {other:?}"),
        }
    }
}

fn decode_hex(seed_id: &str, raw: &str) -> Vec<u8> {
    let compact: String = raw.chars().filter(|character| !character.is_whitespace()).collect();
    assert_eq!(compact.len() % 2, 0, "seed {seed_id}: odd-length hex payload");
    compact
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair).expect("hex payload is ASCII");
            u8::from_str_radix(pair, 16)
                .unwrap_or_else(|error| panic!("seed {seed_id}: invalid hex byte {pair:?}: {error}"))
        })
        .collect()
}

fn observed(bytes: &[u8]) -> Expectation {
    match parse_module(bytes) {
        Err(_) => Expectation::ParseError,
        Ok(module) => match validate(&module) {
            Err(_) => Expectation::ValidationError,
            Ok(()) => Expectation::Valid,
        },
    }
}

#[test]
fn reviewed_fuzz_seed_manifest_replays_exact_stage_expectations() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fuzz/seeds/manifest.tsv");
    let text = fs::read_to_string(&manifest).expect("read reviewed fuzz seed manifest");
    let mut lines = text.lines();
    assert_eq!(
        lines.next(),
        Some("id\ttargets\texpectation\thex\tnote"),
        "unexpected fuzz seed manifest header"
    );

    let mut ids = HashSet::new();
    let mut payloads = HashSet::new();
    let mut count = 0usize;
    let mut saw_valid = false;
    let mut saw_validation_error = false;
    let mut saw_parse_error = false;

    for (offset, line) in lines.enumerate() {
        let line_number = offset + 2;
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(
            fields.len(),
            5,
            "manifest line {line_number} must have exactly five fields"
        );
        let seed_id = fields[0];
        let targets = fields[1];
        let expected = Expectation::parse(fields[2]);
        let bytes = decode_hex(seed_id, fields[3]);

        assert!(
            seed_id
                .chars()
                .all(|character| character.is_ascii_lowercase()
                    || character.is_ascii_digit()
                    || character == '_'
                    || character == '-'),
            "manifest line {line_number}: unsafe seed id {seed_id:?}"
        );
        assert!(ids.insert(seed_id), "duplicate seed id {seed_id:?}");
        assert!(
            payloads.insert(bytes.clone()),
            "duplicate fuzz seed payload for {seed_id:?}"
        );

        let target_set: HashSet<&str> = targets.split(',').collect();
        assert!(!target_set.is_empty(), "seed {seed_id}: empty target set");
        assert!(
            target_set
                .iter()
                .all(|target| matches!(*target, "parse_module" | "parse_validate")),
            "seed {seed_id}: unknown fuzz target in {targets:?}"
        );

        let actual = observed(&bytes);
        assert_eq!(
            actual, expected,
            "seed {seed_id} replay classification changed; bytes={:02x?}",
            bytes
        );

        saw_valid |= expected == Expectation::Valid;
        saw_validation_error |= expected == Expectation::ValidationError;
        saw_parse_error |= expected == Expectation::ParseError;
        count += 1;
    }

    assert!(count >= 10, "reviewed fuzz seed corpus is unexpectedly small");
    assert!(saw_valid, "seed corpus must reach successful validation");
    assert!(
        saw_validation_error,
        "seed corpus must exercise parser-success/validator-rejection paths"
    );
    assert!(saw_parse_error, "seed corpus must exercise parser rejection paths");
}
