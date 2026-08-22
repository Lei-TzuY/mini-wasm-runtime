# Phase 5C — Pinned `f32.wast` tranche

This tranche extends the manifest-driven upstream WAST ingestion with a curated subset of `WebAssembly/spec` `test/core/f32.wast` pinned at commit `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`.

## Provenance and accounting

- upstream repository: `WebAssembly/spec`
- pinned commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- upstream source: `test/core/f32.wast`
- committed fixture: `crates/wasm-runtime/tests/fixtures/phase5c_upstream_f32_subset.wast`
- expected modules: 1
- expected executed assertions: 14
- expected filtered directives: 0

The fixture records both the exact commit and upstream source path. Existing manifest integrity checks reject duplicate source paths, duplicate fixture names, provenance drift, unregistered fixtures, and exact-accounting drift.

## Selected semantics

The selected assertions exercise only instructions already supported by the runtime:

- f32 addition with negative-zero preservation
- addition of the two smallest positive subnormals
- finite subtraction, multiplication, and division
- square root
- `min` / `max` signed-zero selection
- `ceil`, `floor`, and `trunc` around negative fractional inputs
- `nearest` ties-to-even for 2.5 and 3.5
- `nearest(-0.5)` preserving negative zero

Expected ordinary float results are compared by raw f32 bits by the existing WAST runner, so signed-zero regressions cannot be hidden by numeric equality.

## Execution path

The test path remains unchanged:

1. parse the committed WAST fixture using the pinned `wast` dev dependency;
2. encode the selected core module;
3. parse the resulting binary with this repository's parser;
4. validate and instantiate it through `Instance::new`;
5. invoke exports through the public values API;
6. compare results using the existing exact-value / NaN-pattern matcher;
7. require exact per-row module, executed-assertion, and filtered-directive accounting.

## Non-goals

This tranche does not claim support for all of upstream `f32.wast`, all floating-point edge cases, or the complete official WebAssembly spec suite. It does not add or widen any parser, validator, interpreter, host ABI, WAST filter, or CI behavior. Broader manifest coverage remains incremental and fail-closed.
