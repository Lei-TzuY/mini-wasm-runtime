# Phase 5C — pinned `loop.wast` manifest tranche

This tranche extends the manifest-driven WAST ingestion added earlier in Phase 5C with a curated supported subset of upstream `test/core/loop.wast`.

## Provenance

- upstream repository: `WebAssembly/spec`
- pinned commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- upstream source: `test/core/loop.wast`
- committed fixture: `crates/wasm-runtime/tests/fixtures/phase5c_upstream_loop_subset.wast`

The fixture records both the exact commit and upstream source path. The manifest runner rejects duplicate source rows, duplicate fixture registrations, provenance drift, and accounting drift.

## Selected supported semantics

The committed subset exercises only semantics already supported by the runtime:

- loop result values used by `return`, `br`, and `local.set`
- one- and two-parameter loop signatures
- ordered multi-value loop parameters/results
- normal loop completion preserving result vectors
- branch depth through a nested block inside a loop
- branch from a loop body to an outer block carrying one result
- branch from a loop body to an outer block carrying an ordered `[i32, i32, i64]` vector

The branch cases are especially important because WebAssembly loop labels use the loop **parameter vector**, whereas block/if labels use their result vector. The selected cases therefore guard against treating all structured-control labels identically.

## Exact accounting

The manifest row requires:

- modules: 1
- executed assertions: 8
- filtered directives: 0

The existing rows remain unchanged:

- `func.wast`: 3 assertions
- `i32.wast`: 7 assertions
- `memory.wast`: 8 assertions
- `block.wast`: 10 assertions

With this tranche, the manifest-driven regression executes 36 selected pinned upstream assertions total. Any selected assertion becoming filtered is a regression rather than a skipped pass.

## Execution path

The fixture follows the existing ingestion path:

1. parse WAST with the pinned test-only `wast` crate;
2. encode the selected core text module;
3. parse the binary with this repository's `wasm-parser`;
4. validate and instantiate through `wasm-runtime`;
5. invoke exports through the public values API;
6. compare exact result vectors and exact per-row accounting.

## Non-goals

This tranche does not claim support for complete upstream `loop.wast` or for the complete official WebAssembly spec test suite. Unsupported opcodes/proposals are not enabled merely to increase corpus size. Product parser, validator, interpreter, host ABI, dependencies, and CI policy are unchanged.
