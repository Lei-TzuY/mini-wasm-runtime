# Phase 5C pinned `br.wast` context tranche

This tranche expands the manifest-driven pinned upstream WAST corpus with a curated supported subset of `test/core/br.wast`.

## Provenance

- Repository: `WebAssembly/spec`
- Pinned commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- Upstream source: `test/core/br.wast`
- Committed fixture: `crates/wasm-runtime/tests/fixtures/phase5c_upstream_br_subset.wast`

The committed fixture keeps the selected module functions and assertions source-faithful. Unrelated contexts that depend on currently unsupported instructions are omitted instead of being admitted and filtered.

## Selected branch semantics

The row executes 25 assertions with zero filters. The selected cases cover:

- typed branch-result values for `i32`, `i64`, `f32`, and `f64`
- ordered two-value `f64` branch results
- branch exits from `block` and `loop`
- a nested `br` used as the value carried by another `br`
- `br` in the condition position of `br_if`
- single- and multi-value `return` composition
- `br` from an `if` condition, then arm, and else arm
- `br` in the first, middle, last, and all-argument forms of a direct call
- `br` as the operand of `local.set`, `local.tee`, and `global.set`

These vectors stress branch-label result preservation, ordered multi-value transport, unreachable-stack polymorphism, and composition with already-supported control/state-update instructions without adding a new product instruction.

## Deliberately excluded upstream contexts

The full upstream file also contains cases that transitively require instructions outside the current supported product surface, including `drop`, `nop`, `select`, and `br_table`.

Those cases remain outside this tranche. The supported-subset boundary is enforced by fixture selection rather than by adding filters or widening the parser, validator, or interpreter solely for corpus breadth.

## Exact manifest accounting

Before this tranche, the pinned manifest contained 13 unique upstream sources and 246 selected assertions. This tranche adds one source row:

- `test/core/br.wast`
- 1 module
- 25 executed assertions
- 0 filtered directives

The manifest therefore reaches 14 unique upstream sources and 271 selected assertions, all with zero filters.

## Fail-closed contract

This tranche does not modify:

- production parser, validator, interpreter, or host ABI behavior
- WAST directive filtering or expected-value matching
- trap-message mapping
- dependency or Cargo policy
- workflows, warning policy, or CI acceptance rules

The runner change is limited to registering the new committed fixture. If a selected directive starts filtering, if branch result ordering changes, or if control-stack/value-stack composition regresses, the exact manifest accounting test fails.

## Validation

The semantic candidate at `a513cd6b5493ade4ec0c4cb596e6167de858c275` passed CI run `32578792659` on:

- Rust stable / Ubuntu
- Rust stable / Windows
- Rust stable / macOS
- Rust 1.81.0 / Ubuntu (MSRV)

Every matrix job passed:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace --all-targets`
- `cargo doc --workspace --no-deps`

Ubuntu explicitly confirmed `phase5c_wast_ingestion` 3/3 and `pinned_upstream_manifest_executes_with_exact_accounting` passing with the added `br.wast` row.

## Non-goals

This is not a claim of complete `test/core/br.wast` support. Contexts that require unsupported instructions remain excluded until those instructions are independently justified and implemented as product features. The tranche does not alter merge order and must remain stacked on the preceding Phase 5C WAST work.
