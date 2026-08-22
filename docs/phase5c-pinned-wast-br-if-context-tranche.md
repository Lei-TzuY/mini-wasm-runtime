# Phase 5C pinned `br_if.wast` context tranche

This tranche expands the manifest-driven pinned upstream WAST corpus with a curated supported subset of `test/core/br_if.wast`.

## Provenance

- Repository: `WebAssembly/spec`
- Pinned commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- Upstream source: `test/core/br_if.wast`
- Committed fixture: `crates/wasm-runtime/tests/fixtures/phase5c_upstream_br_if_subset.wast`

The fixture keeps the selected module functions and assertions source-faithful. Cases that require unrelated unsupported instructions are left outside the admission boundary rather than accepted and filtered.

## Selected conditional-branch semantics

The row executes 57 assertions with zero filters. The selected cases cover:

- typed `i32`, `i64`, `f32`, and `f64` values carried through taken `br_if` branches
- taken and non-taken `br_if` behavior at the first, middle, and last positions of blocks and loops
- result-carrying block exits and a nested `br_if` consumed by `br`
- `br_if` in another `br_if` condition position
- composition with `return` and both paths of `if`
- conditional branching from direct-call argument positions
- conditional branching from all call-indirect operand positions
- `local.set`, `local.tee`, and `global.set` operand positions
- memory load addresses, store addresses, and store values
- unary, binary, test, and comparison operand positions
- `memory.grow` operand position

Together these vectors exercise both the taken and fallthrough stack rules of `br_if` across control, calls, state updates, memory, and numeric expressions using only already-supported product instructions.

## Deliberately excluded upstream contexts

The full upstream file contains additional cases that depend on currently unsupported instructions such as `drop`, `select`, or `br_table`, including several nested control-expression vectors.

Those cases remain excluded. This tranche does not widen parser, validator, or runtime support merely to increase corpus breadth.

## Exact manifest accounting

Before this tranche, the pinned manifest contained 14 unique upstream sources and 271 selected assertions. This tranche adds:

- `test/core/br_if.wast`
- 1 module
- 57 executed assertions
- 0 filtered directives

The manifest therefore reaches 15 unique upstream sources and 328 selected assertions, all with zero filters.

## Fail-closed contract

This tranche does not modify:

- production parser, validator, interpreter, or host ABI behavior
- WAST directive filtering or expected-value matching
- trap-message mapping
- dependencies or Cargo policy
- workflows, warning policy, or CI acceptance rules

The runner change only registers the committed fixture. Any change that causes a selected directive to filter, changes result values, or breaks taken/fallthrough stack behavior fails the exact manifest-accounting test.

## Validation

The semantic candidate at `edfc88211e8a256936f1844f0d4e8d0d9385e1b3` passed CI run `32579097565` on:

- Rust stable / Ubuntu
- Rust stable / Windows
- Rust stable / macOS
- Rust 1.81.0 / Ubuntu (MSRV)

Every matrix job passed:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace --all-targets`
- `cargo doc --workspace --no-deps`

## Non-goals

This is not a claim of complete `test/core/br_if.wast` support. Unsupported-opcode contexts stay excluded until those instructions are independently justified as product features. The tranche remains stacked on the preceding `br.wast` tranche and does not alter merge order.
