# Phase 5C — Pinned `elem.wast` Active-Element Boundary Tranche

This tranche adds a source-faithful supported subset of `WebAssembly/spec` `test/core/elem.wast` and extends the WAST harness' phase-sensitive negative matching to active element-segment instantiation failures.

## Provenance

- upstream repository: `WebAssembly/spec`
- pinned commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- upstream source: `test/core/elem.wast`
- committed fixture: `phase5c_upstream_elem_subset.wast`

## Selected semantics

Seven successful live modules cover the existing defined-table active-element surface:

- ordinary active initialization at table offset 0
- multiple active segments in one table, including repeated offsets
- executable `call_indirect` checks proving function references at offsets 7 and 9 return 65 and 66
- the final valid slot of a ten-element table
- zero-length element segments at exact offset 0 in zero-sized tables, with and without maximum 0
- a zero-length segment exactly at the end of a twenty-element table

Eight upstream inline-module `assert_trap` directives cover the complementary instantiation bounds failures:

- non-empty segments in zero-sized tables, regardless of declared maximum
- zero-length segment starting beyond a zero-sized table
- non-empty segments starting exactly at current table length even when the declared maximum is larger
- negative i32 offsets interpreted through the WebAssembly 32-bit index domain

One upstream `assert_invalid` case requires an active element segment without any table to encode and structurally parse successfully, then fail static validation specifically as `ValidationError::ElementTableOutOfBounds`.

## Phase-sensitive trap matching

The spec message `out of bounds table access` is mapped to `RuntimeError::ElementSegmentOutOfBounds` for these inline-module instantiation assertions. The runner still requires encoding, parsing, and static validation to succeed before instantiation may fail, so validation failures cannot be mistaken for runtime table-bound traps.

The Differential workspace mirrors one active-element OOB module against Wasmtime. The mini runtime's `ElementSegmentOutOfBounds` and Wasmtime's `TableOutOfBounds` must both normalize to the same table-out-of-bounds class at instantiation.

## Exact accounting

- `elem.wast`: 7 live modules / 11 executed assertions / 0 bare invokes / 0 filters
- full pinned manifest: 32 sources / 985 assertions / 4 invokes -> 33 sources / 996 assertions / 4 invokes
- filters remain zero

## Scope

No runtime product behavior, dependency, MSRV, permanent workflow, warning policy, or CI acceptance rule changes. Imported-table cases, reference-expression element encodings, `global.get`/extended-constant offsets, passive/declarative segments, bulk table instructions, binary-format variants, and unsupported validation messages remain outside this tranche rather than being filtered into apparent success.
