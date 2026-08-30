const UPSTREAM_SPEC_COMMIT: &str = "fc209c5ed8afc4dfeb9252024d217da3376c7a6f";
const SPEC_COMMIT_DECL_PREFIX: &str = "const UPSTREAM_SPEC_COMMIT: &str = \"";

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
        let mut declarations = source.split(SPEC_COMMIT_DECL_PREFIX).skip(1);
        let first = declarations
            .next()
            .and_then(|suffix| suffix.split('"').next());
        assert_eq!(
            first,
            Some(UPSTREAM_SPEC_COMMIT),
            "{path} must declare the pinned WebAssembly/spec revision {UPSTREAM_SPEC_COMMIT}"
        );
        assert!(
            declarations.next().is_none(),
            "{path} must declare exactly one upstream spec revision"
        );
    }
}
