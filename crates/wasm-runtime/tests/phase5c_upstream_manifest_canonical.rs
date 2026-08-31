use std::fs;
use std::path::Path;

const HEADER: [&str; 2] = [
    "# Phase 5C curated upstream WebAssembly/spec provenance inventory.",
    "# path<TAB>full upstream commit",
];

#[test]
fn upstream_manifest_is_canonical_and_deterministic() {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/phase5c_upstream_manifest.tsv");
    let bytes = fs::read(&manifest_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", manifest_path.display()));

    assert!(
        bytes.ends_with(b"\n"),
        "upstream manifest must end with exactly one LF-terminated record"
    );
    assert!(
        !bytes.contains(&b'\r'),
        "upstream manifest must use LF line endings only"
    );

    let source = std::str::from_utf8(&bytes).expect("upstream manifest must remain UTF-8");
    let mut lines = source.lines();
    for expected in HEADER {
        assert_eq!(
            lines.next(),
            Some(expected),
            "upstream manifest canonical header changed"
        );
    }

    let mut previous: Option<&str> = None;
    let mut entries = 0usize;
    for (index, line) in lines.enumerate() {
        let line_number = index + HEADER.len() + 1;
        assert!(!line.is_empty(), "manifest line {line_number} must not be blank");
        assert_eq!(
            line.trim(),
            line,
            "manifest line {line_number} has leading or trailing whitespace"
        );

        let mut fields = line.split('\t');
        let path = fields
            .next()
            .expect("split always yields at least the path field");
        let revision = fields
            .next()
            .unwrap_or_else(|| panic!("manifest line {line_number} is missing its revision"));
        assert!(
            fields.next().is_none(),
            "manifest line {line_number} must contain exactly one tab separator"
        );
        assert!(!path.is_empty(), "manifest line {line_number} has an empty path");
        assert!(
            !revision.is_empty(),
            "manifest line {line_number} has an empty revision"
        );

        if let Some(previous) = previous {
            assert!(
                previous < path,
                "manifest entries must be strictly lexicographically ordered: {previous:?} before {path:?}"
            );
        }
        previous = Some(path);
        entries += 1;
    }

    assert!(entries > 0, "upstream manifest must contain at least one entry");
}
