use std::fs;
use std::path::Path;

const UPSTREAM_SPEC_COMMIT: &str = "fc209c5ed8afc4dfeb9252024d217da3376c7a6f";

fn full_hex_commits(source: &str) -> impl Iterator<Item = &str> {
    source
        .split(|character: char| !character.is_ascii_hexdigit())
        .filter(|token| token.len() == 40)
}

fn is_phase5c_spec_vector(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    name.starts_with("phase5c_") && name.ends_with("_spec_vectors.rs")
}

#[test]
fn all_phase5c_spec_vectors_remain_pinned_to_one_revision() {
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);
    assert!(
        UPSTREAM_SPEC_COMMIT
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit()),
        "upstream spec revision must remain a full hexadecimal commit id"
    );

    let tests_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut paths: Vec<_> = fs::read_dir(&tests_dir)
        .expect("wasm-runtime tests directory must be readable")
        .map(|entry| {
            entry
                .expect("tests directory entry must be readable")
                .path()
        })
        .filter(|path| is_phase5c_spec_vector(path))
        .collect();
    paths.sort();

    assert!(
        !paths.is_empty(),
        "Phase 5C spec-vector corpus must not silently disappear"
    );

    for path in paths {
        let source = fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!("failed to read {}: {error}", path.display());
        });
        let commits: Vec<_> = full_hex_commits(&source).collect();
        assert_eq!(
            commits,
            [UPSTREAM_SPEC_COMMIT],
            "{} must contain exactly the pinned spec revision",
            path.display()
        );
    }
}
