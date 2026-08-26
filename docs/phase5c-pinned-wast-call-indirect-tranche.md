# Phase 5C pinned WAST call_indirect tranche

## Provenance

This tranche is pinned to `WebAssembly/spec@fc209c5ed8afc4dfeb9252024d217da3376c7a6f` and derives from `test/core/call_indirect.wast`.

The committed fixture keeps the complete first upstream module, then selects the first 51 executable `assert_return` / `assert_trap` directives and stops immediately before the standalone factorial/Fibonacci recursion assertion block. Exact accounting is **1 module / 51 executed assertions / 0 filtered directives**.

## Coverage

The selected assertions exercise:

- i32/i64/f32/f64 indirect-call results and arguments
- multi-value indirect-call results and arguments
- explicit type-index dispatch
- first/second/mixed parameter positions
- dynamic dispatch over multiple concrete function signatures
- structural function-type equivalence via duplicate type declarations
- out-of-range and negative table selectors normalized as upstream `undefined element`
- dynamic signature mismatch normalized as upstream `indirect call type mismatch`
- bounded recursive dispatch reached transitively by the selected dispatch vectors

The runner now recognizes `indirect call type mismatch` as the existing `RuntimeError::IndirectCallTypeMismatch` class. This changes conformance normalization only; interpreter behavior is unchanged.

## Explicit exclusions

Standalone factorial/Fibonacci recursion matrices, mutual recursion, `assert_exhaustion`, later instruction-composition assertions, multi-table proposal coverage, and malformed/invalid directives are not selected in this tranche. They are excluded from the committed fixture rather than silently filtered. The broader WAST roadmap item remains open.

## Accounting contract

The manifest records one row for the pinned source and requires exact module/assertion/filter totals. The fixture embeds both pinned commit and original source path. Duplicate sources/fixtures, provenance drift, selected assertion filtering, or trap-class drift fail the existing manifest regression.

## Validation

```text
cargo fmt --all
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p wasm-runtime --test phase5c_wast_ingestion
cargo test --workspace
cargo doc --workspace --no-deps
```
