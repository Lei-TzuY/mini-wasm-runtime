# Phase 5C — Pinned `f64_cmp.wast` tranche

This tranche expands the manifest-driven upstream WAST corpus with an already-supported f64 comparison slice. It is tests/docs-only and does not widen parser, validator, interpreter, host, dependency, or CI acceptance.

## Provenance

- upstream repository: `WebAssembly/spec`
- pinned commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- upstream source: `test/core/f64_cmp.wast`
- committed fixture: `crates/wasm-runtime/tests/fixtures/phase5c_upstream_f64_cmp_subset.wast`

The fixture records both the exact commit and source path. Existing manifest integrity checks reject duplicate sources/fixtures, provenance drift, malformed accounting, or an unregistered fixture.

## Selected semantics

The curated subset exercises all six f64 comparison operators (`eq`, `ne`, `lt`, `le`, `gt`, `ge`) over representative WebAssembly/IEEE-754 edge behavior:

- `-0.0 == +0.0`
- `-0.0 != +0.0` is false
- ordered finite comparisons
- reflexive ordering for infinities
- NaN is unordered for `eq`, `lt`, and `gt`
- NaN makes `ne` true

## Exact accounting

This row requires exactly:

- 1 encoded/instantiated module
- 12 executed assertions
- 0 filtered directives

A selected assertion becoming filtered is therefore a regression rather than a skipped pass. With the preceding ten manifest rows unchanged, the manifest regression executes 108 selected pinned upstream assertions across eleven sources.

## Execution path

The fixture follows the established WAST ingestion path: parse through the pinned `wast` dev dependency, encode core text modules, parse bytes with this repository's parser, validate/instantiate with `Instance::new`, then invoke exports through the public runtime API. The test front end is external; the implementation under test remains this repository's parser, validator, and interpreter.

## Non-goals

This tranche does not claim complete acceptance of upstream `test/core/f64_cmp.wast` or the complete WebAssembly spec suite. It intentionally selects cases already inside the implemented surface and leaves broader manifest expansion open.
