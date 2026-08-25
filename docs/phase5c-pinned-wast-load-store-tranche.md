# Phase 5C pinned WAST load/store tranche

This tranche extends the manifest-driven upstream WAST corpus with source-faithful executable subsets from the pinned WebAssembly core spec tests:

- upstream repository: `WebAssembly/spec`
- pinned commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- `test/core/load.wast`: 1 module, 37 executed `assert_return` directives
- `test/core/store.wast`: 1 module, 9 executed `assert_return` directives
- filtered directives: 0

Together these add 46 assertions, taking the committed manifest from 19 sources / 533 assertions to 21 sources / 579 assertions.

## Covered semantics

The selected `load.wast` module keeps the upstream instruction-composition cases where `i32.load` and narrow loads appear as operands or values of branches, `return`, `if`, untyped `select`, direct and indirect calls, local/global writes, nested loads/stores, integer unary/binary/test/comparison operators, and `memory.grow`. The selected `store.wast` module keeps the upstream void-valued store cases embedded in `block`, `loop`, branches, `return`, and `if`.

The fixtures are intentionally source-faithful: function bodies and selected assertions are copied from the pinned upstream files except for provenance comments added at the top.

## Explicit exclusions

The remainder of the two upstream files is not silently filtered. It is outside this tranche because it consists of `assert_malformed` / `assert_invalid` validation-shape tests that are not part of the current module + `assert_return` / `assert_trap` WAST ingestion contract. Proposal-dependent memory forms remain out of scope as documented by the project boundaries.

The broad roadmap item for remaining supported numeric/control/memory WAST coverage therefore remains open.

## Accounting contract

`phase5c_upstream_manifest.tsv` records exact module, executed-assertion, and filtered-directive counts. The ingestion test rejects duplicate upstream sources or fixtures, requires every fixture to record the pinned commit and source path, and fails on any accounting drift. Both new rows require zero filtered directives.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p wasm-runtime --test phase5c_wast_ingestion
cargo test --workspace
cargo doc --workspace --no-deps
```
