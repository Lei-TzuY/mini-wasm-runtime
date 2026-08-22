# Phase 5C — Pinned `memory_grow.wast` Tranche

This tranche extends the manifest-driven pinned upstream WAST corpus with a distinct `WebAssembly/spec` `test/core/memory_grow.wast` source. It adds conformance evidence for semantics the runtime already implements; it does not widen the supported WebAssembly surface.

## Provenance

- upstream repository: `WebAssembly/spec`
- pinned commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- upstream source: `test/core/memory_grow.wast`
- committed fixture: `phase5c_upstream_memory_grow_subset.wast`

## Selected upstream slice

The fixture keeps the upstream `(memory 0 10)` module and its eight assertions. This slice was chosen because it exercises the stateful semantics of `memory.grow` without coupling the regression to large host allocations or long byte-scanning loops.

The assertions prove that:

- `memory.grow 0` returns the previous/current size without changing memory size;
- successful non-zero growth returns the previous page count;
- successive growth composes correctly until the declared maximum is reached;
- `memory.grow 0` at the maximum still succeeds and reports the maximum;
- growth beyond the declared maximum returns `-1` and does not trap;
- an oversized `0x10000`-page request also returns `-1`.

The ordered sequence is important: the expected values depend on state changes produced by earlier assertions, so the fixture detects implementations that return the new size instead of the previous size or mutate memory after a failed growth request.

## Exact accounting

This adds one manifest row with:

- 1 module
- 8 executed assertions
- 0 filtered directives

The pinned manifest therefore increases from 206 to 214 selected assertions and from 12 to 13 unique upstream source rows. Every manifest row continues to require zero filters.

## Fail-closed boundary

No WAST filter, trap mapper, expected-value matcher, parser, validator, interpreter, host ABI, dependency, workflow, warning policy, or CI acceptance rule changes in this tranche. The only runner change is registering the new committed fixture.

The existing manifest regression still requires exact provenance, unique source/fixture names, exact module count, exact executed-assertion count, and exact filter count. A selected assertion becoming filtered or a result changing cannot silently reduce coverage.

## Validation boundary

The fixture/manifest/registry semantic candidate passed the complete GitHub Actions matrix before this document was added:

- Rust stable / Ubuntu
- Rust stable / Windows
- Rust stable / macOS
- Rust 1.81.0 / Ubuntu

Ubuntu logs explicitly confirmed `phase5c_wast_ingestion` 3/3 and `pinned_upstream_manifest_executes_with_exact_accounting` passing with the new `memory_grow.wast` row. Final validation is rerun on the documentation-inclusive HEAD before the PR is sealed.

## Non-goals

This tranche does not claim complete `memory_grow.wast` coverage. In particular, it intentionally leaves the large unbounded-growth sequence, byte-by-byte newly-allocated-memory zeroing loop, broader memory-boundary module, and control-expression embedding cases for separate reviewable slices. It does not change runtime allocation policy or add any new opcode.
