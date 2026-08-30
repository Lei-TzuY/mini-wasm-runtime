use std::fs;
use std::path::Path;

const UPSTREAM_SPEC_COMMIT: &str = "fc209c5ed8afc4dfeb9252024d217da3376c7a6f";
const PINNED_BASELINE: &[&str] = &[
    "phase5c_supported_spec_vectors.rs",
    "phase5c_supported_arithmetic_spec_vectors.rs",
    "phase5c_supported_select_spec_vectors.rs",
    "phase5c_negative_global_spec_vectors.rs",
    "phase5c_supported_control_spec_vectors.rs",
    "phase5c_supported_reinterpret_spec_vectors.rs",
    "phase5c_supported_promote_demote_rounding_spec_vectors.rs",
    "phase5c_negative_segment_mode_spec_vectors.rs",
    "phase5c_negative_segment_offset_spec_vectors.rs",
    "phase5c_negative_segment_target_spec_vectors.rs",
    "phase5c_imported_segment_boundary_spec_vectors.rs",
    "phase5c_supported_active_data_spec_vectors.rs",
    "phase5c_supported_active_data_boundary_spec_vectors.rs",
    "phase5c_supported_active_element_spec_vectors.rs",
];

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

fn has_upstream_provenance_marker(source: &str) -> bool {
    source.contains("UPSTREAM_SPEC_COMMIT") || source.contains("WebAssembly/spec")
}

fn assert_pinned(path: &Path) {
    let source = fs::read_to_string(path).unwrap_or_else(|error| {
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

#[test]
fn documented_phase5c_upstream_vectors_remain_pinned_to_one_revision() {
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);
    assert!(
        UPSTREAM_SPEC_COMMIT
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit()),
        "upstream spec revision must remain a full hexadecimal commit id"
    );

    let tests_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    for name in PINNED_BASELINE {
        assert_pinned(&tests_dir.join(name));
    }

    let entries = fs::read_dir(&tests_dir).expect("wasm-runtime tests directory must be readable");
    for entry in entries {
        let path = entry.expect("tests directory entry must be readable").path();
        if !is_phase5c_spec_vector(&path) {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!("failed to read {}: {error}", path.display());
        });
        if has_upstream_provenance_marker(&source) {
            assert_pinned(&path);
        }
    }
}
