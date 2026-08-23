# Differential regression fixtures

This directory contains a small seeded corpus of manually minimized reproducer modules for semantics already supported by the mini runtime. These fixtures are regression guards and replay examples; they are not claims that each case corresponds to a previously observed production bug.

`manifest.tsv` has exactly four tab-separated fields per non-comment row:

```text
id	fixture	outcome_kind	expected
```

Supported outcome kinds are `i32`, `i64`, `pair_i32_i64`, and `trap`. Trap expectations use the normalized semantic classes understood by `differential/tests/regressions.rs`.

The replay harness rejects duplicate IDs or fixture paths, unsafe/non-WAT paths, missing files, malformed rows, unknown kinds/classes, validation or instantiation failures where an execution trap was expected, and any result or trap that does not match both the manifest and Wasmtime.

Generated i32 differentials can automatically minimize a real mini-vs-Wasmtime mismatch into `differential/target/differential-captures/`. Each capture contains a minimized `.wat`, one `manifest.tsv`-compatible row, and provenance metadata. Differential CI uploads these files on failure when they exist. Capture is intentionally staging-only: CI never edits this committed corpus.

Before promotion, review the independent-model agreement and minimized behavior, copy the `.wat` into this directory, append the supplied manifest row, keep the generated ID stable, and rerun the full differential suite. Record only the smallest observable behavior needed to prevent recurrence.
