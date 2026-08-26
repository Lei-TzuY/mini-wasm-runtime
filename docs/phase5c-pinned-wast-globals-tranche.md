# Phase 5C pinned WAST globals tranche

## Provenance

This tranche is pinned to `WebAssembly/spec@fc209c5ed8afc4dfeb9252024d217da3376c7a6f` and derives from `test/core/global.wast`.

The committed fixture keeps the upstream numeric global imports and defined numeric globals in their original index order, then retains the supported getter/setter and composition functions/assertions. It also includes the two source-faithful mutable-f32-global export modules from later in the source.

Accounting: 3 modules, 52 executed assertions, 0 filtered directives.

## Spectest bindings

The ingestion runner now supplies deterministic standard test-environment bindings for:

- `spectest.global_i32 = 666`
- `spectest.global_i64 = 666`

These are test-harness bindings only. They use the runtime's existing immutable numeric-global host API and do not change product behavior or make arbitrary imports implicit.

## Coverage

The executable tranche covers immutable and mutable i32/i64/f32/f64 globals, imported numeric-global index ordering, persistent mutation across repeated invocations, global values used by select/loop/if/branches, direct and indirect calls, memory load/store/grow, local.set/local.tee/global.set, and numeric unary/binary/comparison operands. The upstream undefined-element indirect-call trap remains normalized by the existing WAST runner.

## Explicit exclusions

The upstream `global.wast` source also contains reference-valued globals and assertions, globals initialized through `global.get` or richer constant expressions, plus `assert_invalid`/`assert_malformed` negative-validation cases. Those forms remain outside this positive WAST ingestion tranche and are omitted rather than silently filtered. Existing dedicated negative-conformance tests continue to cover the fail-closed boundary, and the broad WAST roadmap item remains open.

## Validation

```text
cargo fmt --all
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p wasm-runtime --test phase5c_wast_ingestion
cargo test --workspace
cargo doc --workspace --no-deps
```
