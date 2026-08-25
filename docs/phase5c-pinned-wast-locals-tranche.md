# Phase 5C pinned WAST locals tranche

## Provenance

This tranche is pinned to `WebAssembly/spec@fc209c5ed8afc4dfeb9252024d217da3376c7a6f` and takes the executable prefix before the first upstream invalid-typing section from:

- `test/core/local_get.wast`: 1 module, 19 `assert_return` directives
- `test/core/local_set.wast`: 1 module, 19 `assert_return` directives
- `test/core/local_tee.wast`: 1 module, 55 `assert_return` directives

Total: 3 modules, 93 executed assertions, 0 filtered directives.

## Coverage

The tranche covers zero initialization of numeric locals, parameter/local indexing across i32/i64/f32/f64, local mutation and tee result semantics, and local values composed through structured control, branches, select, direct and indirect calls, globals, memory load/store/grow, numeric operators, conversions, and exact NaN payload/sign behavior already supported by the runtime.

`local_tee.wast` is intentionally valuable as a composition stressor: it verifies that tee both mutates the selected local and leaves the exact typed value on the operand stack while surrounding instructions consume it.

## Explicit exclusions

The remaining upstream sections are `assert_invalid` validation cases, plus proposal/reference-type cases beyond this runner's current module + `assert_return` / `assert_trap` ingestion contract. They are excluded from the committed fixture rather than silently filtered. The broader remaining WAST roadmap item therefore stays open.

## Accounting contract

The manifest records one row per pinned upstream source and requires exact module/assertion/filter totals. Each committed fixture embeds both the pinned commit and original source path, and the runner rejects duplicate manifest sources/fixtures or accounting drift.

## Validation

Run:

```text
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p wasm-runtime --test phase5c_wast_ingestion
cargo test --workspace
cargo doc --workspace --no-deps
```
