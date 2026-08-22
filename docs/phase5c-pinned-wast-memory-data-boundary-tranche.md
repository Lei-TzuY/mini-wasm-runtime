# Phase 5C — Pinned `memory.wast` Data/Boundary Tranche

This tranche extends the existing pinned `WebAssembly/spec` `test/core/memory.wast` fixture without widening the runtime surface.

## Provenance

- upstream repository: `WebAssembly/spec`
- pinned commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- upstream source: `test/core/memory.wast`
- committed fixture: `phase5c_upstream_memory_subset.wast`

## Added semantics

The existing eight narrow load/store assertions remain unchanged. Six source-faithful assertions add two distinct memory invariants.

### Active data and mixed typed memory

The first module now includes the upstream active data segments at offsets 0 and 20 and checks both initialized bytes and zero-filled gaps. The upstream `cast` vector also exercises full-width i64/f64 storage, reinterpretation, explicitly under-aligned memory operations that are still valid under WebAssembly alignment rules, and exact reconstruction of `42.0`.

### One-page boundary behavior

A second upstream-derived module exports one 64-KiB memory plus unrelated globals. Loads at offsets 0, 10000, 60000, and 65535 must all succeed and return zero. The last case locks the final valid byte of a one-page memory and catches off-by-one bounds regressions; exported globals must not affect memory indexing or bounds.

## Exact accounting

The `memory.wast` manifest row changes from:

- 1 module
- 8 executed assertions
- 0 filtered directives

to:

- 2 modules
- 14 executed assertions
- 0 filtered directives

Across the existing twelve unique source rows, the pinned manifest increases from 200 to 206 selected assertions, all with zero filters.

## Fail-closed boundary

No WAST runner/filter/trap mapper, parser, validator, interpreter, host ABI, dependency, workflow, warning policy, or CI acceptance rule changes in this tranche. Any selected assertion becoming filtered, either module failing to instantiate, or any expected value changing fails the manifest regression.

The fixture/manifest-only semantic candidate passed Ubuntu stable, macOS stable, and Rust 1.81.0 Ubuntu before this document was added; Windows validation was still completing. Final validation is rerun on the documentation-inclusive HEAD before the PR is sealed.

## Non-goals

This does not claim complete `memory.wast` support. It does not add new memory instructions, multiple memories, memory64, bulk memory, or any new WAST directive support.
