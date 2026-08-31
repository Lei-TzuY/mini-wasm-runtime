use std::collections::HashSet;
use std::fs;
use std::path::Path;

const UPSTREAM_SPEC_COMMIT: &str = "fc209c5ed8afc4dfeb9252024d217da3376c7a6f";

fn full_hex_commits(source: &str) -> impl Iterator<Item = &str> {
    source
        .split(|character: char| !character.is_ascii_hexdigit())
        .filter(|token| token.len() == 40)
}

#[test]
fn curated_upstream_manifest_is_complete_and_self_consistent() {
    let tests_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let manifest_path = tests_dir.join("fixtures/phase5c_upstream_manifest.tsv");
    let manifest = fs::read_to_string(&manifest_path).unwrap_or_else(|error| {
        panic!("failed to read {}: {error}", manifest_path.display());
    });

    let mut seen = HashSet::new();
    let mut entries = 0usize;

    for (line_index, raw_line) in manifest.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (name, commit) = line.split_once('\t').unwrap_or_else(|| {
            panic!(
                "{}:{} must contain exactly a filename and full commit separated by a tab",
                manifest_path.display(),
                line_index + 1
            );
        });
        assert!(
            !name.contains('/') && !name.contains('\\'),
            "{}:{} must name a test file, not an arbitrary path",
            manifest_path.display(),
            line_index + 1
        );
        assert!(
            name.starts_with("phase5c_") && name.ends_with("_spec_vectors.rs"),
            "{}:{} contains a non-Phase-5C spec-vector filename",
            manifest_path.display(),
            line_index + 1
        );
        assert_eq!(
            commit, UPSTREAM_SPEC_COMMIT,
            "{}:{} is not pinned to the Phase 5C upstream revision",
            manifest_path.display(),
            line_index + 1
        );
        assert!(
            commit.len() == 40 && commit.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "{}:{} must use a full hexadecimal commit id",
            manifest_path.display(),
            line_index + 1
        );
        assert!(
            seen.insert(name.to_owned()),
            "{}:{} duplicates manifest entry {name}",
            manifest_path.display(),
            line_index + 1
        );

        let vector_path = tests_dir.join(name);
        let source = fs::read_to_string(&vector_path).unwrap_or_else(|error| {
            panic!("manifest entry {} is unreadable: {error}", vector_path.display());
        });
        let commits: Vec<_> = full_hex_commits(&source).collect();
        assert_eq!(
            commits,
            [commit],
            "{} must contain exactly the manifest-pinned upstream revision",
            vector_path.display()
        );
        entries += 1;
    }

    assert!(entries > 0, "upstream provenance manifest must not be empty");
}
