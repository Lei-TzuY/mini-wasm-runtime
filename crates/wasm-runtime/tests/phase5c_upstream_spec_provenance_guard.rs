const UPSTREAM_SPEC_COMMIT: &str = "fc209c5ed8afc4dfeb9252024d217da3376c7a6f";

const CURATED_SPEC_SOURCES: &[(&str, &str)] = &[
    (
        "phase5c_supported_spec_vectors.rs",
        include_str!("phase5c_supported_spec_vectors.rs"),
    ),
    (
        "phase5c_supported_arithmetic_spec_vectors.rs",
        include_str!("phase5c_supported_arithmetic_spec_vectors.rs"),
    ),
    (
        "phase5c_supported_select_spec_vectors.rs",
        include_str!("phase5c_supported_select_spec_vectors.rs"),
    ),
    (
        "phase5c_negative_global_spec_vectors.rs",
        include_str!("phase5c_negative_global_spec_vectors.rs"),
    ),
];

fn full_hex_commits(source: &str) -> impl Iterator<Item = &str> {
    source
        .split(|character: char| !character.is_ascii_hexdigit())
        .filter(|token| token.len() == 40)
}

#[test]
fn curated_upstream_spec_vectors_remain_pinned_to_one_revision() {
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);
    assert!(
        UPSTREAM_SPEC_COMMIT
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit()),
        "upstream spec revision must remain a full hexadecimal commit id"
    );

    for (path, source) in CURATED_SPEC_SOURCES {
        let commits: Vec<_> = full_hex_commits(source).collect();
        assert_eq!(
            commits,
            [UPSTREAM_SPEC_COMMIT],
            "{path} must record exactly the pinned WebAssembly/spec revision {UPSTREAM_SPEC_COMMIT}"
        );
    }
}
