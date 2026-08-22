# Phase 5C pinned `block.wast` tranche

This tranche extends the existing manifest-driven WAST ingestion with a curated supported subset from `WebAssembly/spec` commit `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`, source `test/core/block.wast`.

## Why this tranche

Previous manifest rows covered multi-value function results, i32 arithmetic/traps, and narrow integer memory loads/stores. `block.wast` adds independent evidence for already-implemented structured-control semantics without expanding the product acceptance surface.

## Selected semantics

The committed fixture exercises:

- single-result blocks and nested blocks
- `br` carrying a single result
- `br` carrying an ordered multi-value result vector
- block parameters consumed by instructions inside the block
- multiple block parameters
- identity blocks that preserve parameter order as multiple results
- branches from parameterized blocks while preserving the declared result vector

These cases intentionally stress the control-stack/result-vector boundary that also underpins the runtime's multi-value implementation.

## Provenance and accounting

Manifest row:

- upstream repository: `WebAssembly/spec`
- pinned commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- source: `test/core/block.wast`
- fixture: `phase5c_upstream_block_subset.wast`
- expected modules: 1
- expected executed assertions: 10
- expected filtered directives: 0

The existing manifest integrity rules require unique source and fixture names, the exact pinned commit, source-path provenance recorded in the fixture, and exact per-row module/executed/filter accounting. A selected assertion becoming filtered is therefore a regression rather than a skipped pass.

## Execution path

The fixture is parsed by the pinned test-only `wast` dependency, encoded as core Wasm, parsed by this repository's parser, validated/instantiated by `Instance::new`, and executed through `invoke_export_values`. The `wast` crate is only the text/script front end; it does not provide the runtime semantics under test.

## Non-goals

This tranche does not claim support for complete `test/core/block.wast`, complete official spec-test ingestion, unsupported directives, reference types, bulk memory/table operations, SIMD, threads, memory64, or multi-memory/multi-table. It changes no parser, validator, interpreter, host ABI, dependency, or CI policy.
