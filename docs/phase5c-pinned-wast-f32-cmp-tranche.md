# Phase 5C — Pinned `f32_cmp.wast` tranche

## Provenance

This tranche is derived from `WebAssembly/spec` commit `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`, source `test/core/f32_cmp.wast`.

The committed fixture is intentionally a curated supported subset rather than a claim that the complete upstream file is accepted. CI stays network-independent while the manifest records exact source provenance and accounting.

## Selected semantics

The fixture exercises all six core f32 comparison operators: `eq`, `ne`, `lt`, `le`, `gt`, and `ge`.

The selected assertions cover:

- `-0.0 == +0.0`
- `-0.0 != +0.0` being false
- ordered finite comparisons on negative and positive values
- reflexive ordering for infinities
- unordered NaN behavior: `eq`, `lt`, and `gt` are false while `ne` is true

These cases protect IEEE/WebAssembly comparison semantics without adding any new runtime opcode or acceptance surface.

## Accounting

Manifest row:

- modules: 1
- executed assertions: 12
- filtered directives: 0

With the nine existing manifest rows unchanged, the runner now executes 96 selected pinned upstream assertions across ten source rows, all with zero filters.

## Fail-closed contract

Existing ingestion rules remain unchanged: exact pinned commit, unique source and fixture names, fixture-recorded provenance, exact module/executed/filter counts, and explicit typed filtering for unsupported forms. A selected assertion becoming filtered or changing result semantics is a regression.

## Non-goals

This tranche does not claim support for the complete `test/core/f32_cmp.wast`, the complete WebAssembly core test suite, reference types, SIMD, threads, memory64, or any proposal outside the repository's current supported surface.
