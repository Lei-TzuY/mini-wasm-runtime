# Phase 5C pinned WAST parametric/control tranche

This tranche expands the manifest-driven pinned upstream WebAssembly spec corpus with source-faithful supported subsets for `nop`, untyped numeric `select`, and `br_table`.

## Provenance

All three fixtures come from `WebAssembly/spec` at the repository's existing pinned commit:

- commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- `test/core/nop.wast` -> `crates/wasm-runtime/tests/fixtures/phase5c_upstream_nop_subset.wast`
- `test/core/select.wast` -> `crates/wasm-runtime/tests/fixtures/phase5c_upstream_select_subset.wast`
- `test/core/br_table.wast` -> `crates/wasm-runtime/tests/fixtures/phase5c_upstream_br_table_subset.wast`

The selected function bodies and assertions are kept source-faithful. Unsupported portions of each upstream file are omitted from the committed subset rather than admitted and silently filtered by the WAST runner.

## Selected semantics

### `nop.wast`

The row executes 37 assertions covering `nop` at function, `drop`, `select`, block, loop, `if`, `br`, `br_if`, and `br_table` positions. These cases check that `nop` preserves surrounding stack/control behavior instead of accidentally consuming or synthesizing values.

### `select.wast`

The row executes 72 assertions over the supported untyped numeric `select` instruction (`0x1b`):

- i32/i64/f32/f64 value selection for zero and nonzero conditions
- exact selected NaN payload preservation for f32/f64
- nested `select`
- composition with loop, `if`, `br_if`, and `br_table`
- composition with `drop`, `br`, local/global mutation, integer tests/arithmetic/comparisons, and i64-to-i32 wrapping conversion

### `br_table.wast`

The row executes 52 assertions covering:

- unreachable-stack polymorphism around i32/i64/f32/f64 consumers
- typed branch-result transport for all four numeric value types
- default dispatch with empty and value-carrying labels
- one-entry indexed/default dispatch
- multi-target dispatch across nested blocks
- value transport through nested local updates
- negative and all-ones i32 selectors, exercising the runtime's unsigned-selector interpretation

## Exact accounting

Before this tranche the pinned manifest contained 16 unique upstream sources and 372 selected assertions. The three new rows add 161 assertions:

- `nop.wast`: 1 module, 37 assertions, 0 filters
- `select.wast`: 1 module, 72 assertions, 0 filters
- `br_table.wast`: 1 module, 52 assertions, 0 filters

The manifest therefore reaches 19 unique upstream sources and 533 selected assertions, with zero filtered directives in every row.

## Deliberately excluded upstream surface

This is still curated supported-surface conformance rather than a claim that the complete upstream files are accepted.

- typed `select` (`0x1c`) remains outside the runtime's documented surface
- reference-typed select/join cases are excluded
- invalid-module assertions remain outside the current `assert_return`/`assert_trap` ingestion contract
- the enormous `br_table` stress vector is excluded from this deterministic semantic tranche
- reference-type meet cases and other proposal-dependent contexts are excluded

The general roadmap item for broader remaining numeric/control/memory WAST coverage therefore stays open.

## Fail-closed contract

The tranche changes no production parser, validator, interpreter, host ABI, dependency, MSRV, or workflow behavior. Runner logic is unchanged apart from registering the three fixtures and their manifest rows.

If any selected directive starts filtering, if a fixture loses its pinned provenance marker, if source/fixture uniqueness drifts, or if execution semantics change, `pinned_upstream_manifest_executes_with_exact_accounting` fails.

## Validation

The tranche is expected to pass the ordinary repository gate:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo doc --workspace --no-deps
```

The focused conformance check is:

```text
cargo test -p wasm-runtime --test phase5c_wast_ingestion -- --nocapture
```
