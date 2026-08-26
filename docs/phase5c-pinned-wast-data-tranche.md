# Phase 5C — Pinned `data.wast` Active-Data Boundary Tranche

This tranche adds a source-faithful supported subset of `WebAssembly/spec` `test/core/data.wast` and extends the WAST harness' phase-sensitive trap normalization for active data-segment instantiation failures.

## Provenance

- upstream repository: `WebAssembly/spec`
- pinned commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- upstream source: `test/core/data.wast`
- committed fixture: `phase5c_upstream_data_subset.wast`

## Selected semantics

Seven successful module directives cover defined-memory active data boundaries without adding imports or richer constant expressions:

- one-page data at byte `0` and the final valid byte `0xffff`
- two-page data at the final valid byte `0x1ffff`
- zero-page memory with an empty segment at exact offset `0`
- zero-page memory with declared maximum `0` and an empty segment at exact offset `0`
- one-page memory with an empty segment at exact end `0x1_0000`
- concatenated empty data strings on zero-page memories, with and without explicit maximum `0`

Ten upstream `assert_trap` directives then pin the complementary instantiation-time bounds failures:

- non-empty data in zero-page memories, regardless of declared maximum
- zero-length data at offset `1` in zero-page memories
- offsets exactly one byte beyond one- and two-page current lengths
- declared maxima larger than the current size do not make active initialization in-bounds
- negative i32 offsets are interpreted through the WebAssembly 32-bit address domain and remain out of bounds

One upstream `assert_invalid` case requires a data segment without any memory to pass binary parsing and fail specifically in static validation as `ValidationError::DataMemoryOutOfBounds`.

## Phase-sensitive trap normalization

The spec uses the message `out of bounds memory access` for both dynamic memory instructions and active segment initialization. The runner therefore maps that spec trap class to either `RuntimeError::MemoryOutOfBounds` during execution or `RuntimeError::DataSegmentOutOfBounds` during inline-module instantiation. The existing `assert_trap (module ...)` path still requires encoding, parsing, and static validation to succeed before instantiation may fail, so phase boundaries are not weakened.

The isolated Differential workspace mirrors one active-data OOB module against Wasmtime and requires both engines to normalize the instantiation failure to the memory-out-of-bounds trap class.

## Exact accounting

- `data.wast`: 7 live modules / 11 executed assertions / 0 invokes / 0 filters
- full pinned manifest: 31 sources / 974 assertions / 4 invokes -> 32 sources / 985 assertions / 4 invokes
- filters remain zero

## Scope

No runtime product behavior, dependency, MSRV, permanent workflow, warning policy, or CI acceptance rule changes. Imported-memory `data.wast` cases, `global.get`/extended-constant offsets, passive/bulk-memory directives, binary explicit-index negative cases, and unsupported validation messages remain outside this tranche rather than being filtered into apparent success.
