# Phase 5C — Pinned Memory WAST Manifest Tranche

This tranche extends the manifest-driven upstream WAST ingestion introduced in earlier Phase 5C work into linear-memory semantics without changing parser, validator, interpreter, host ABI, dependencies, or CI policy.

## Provenance

- upstream repository: `WebAssembly/spec`
- pinned commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- upstream source: `test/core/memory.wast`
- committed curated fixture: `crates/wasm-runtime/tests/fixtures/phase5c_upstream_memory_subset.wast`

The fixture records both the exact commit and upstream source path. The manifest runner rejects duplicate source paths, duplicate fixture registrations, provenance drift, and accounting drift.

## Selected supported semantics

The curated subset takes supported assertions from upstream `memory.wast` that exercise narrow integer stores followed by signed and unsigned loads:

- `i32.store8` with `i32.load8_s` / `i32.load8_u`
- `i32.store16` with `i32.load16_s` / `i32.load16_u`
- `i64.store8` with `i64.load8_s` / `i64.load8_u`
- `i64.store32` with `i64.load32_s` / `i64.load32_u`

The selected values prove truncation to the stored width and the distinction between sign extension and zero extension on load, including high-bit-set patterns.

## Exact accounting

The memory manifest row requires exactly:

- 1 module
- 8 executed assertions
- 0 filtered directives

Together with the existing `func.wast` and `i32.wast` rows, the manifest-driven regression executes 18 selected pinned upstream assertions. A selected assertion becoming filtered, a module disappearing, or a fixture/source mapping drifting causes a test failure.

## Execution path

The fixture is parsed by the pinned test-only `wast` frontend, encoded to Wasm bytes, parsed by this repository's `wasm_parser`, validated/instantiated by the repository's own runtime path, and executed through the public export invocation API. No reference runtime decides the result.

## Non-goals

This tranche does not claim support for the complete upstream `memory.wast` file or the complete official spec suite. It does not add unsupported validation directives, multi-memory, memory64, threads/shared-memory proposal semantics, bulk-memory proposal execution, or any new production capability.
