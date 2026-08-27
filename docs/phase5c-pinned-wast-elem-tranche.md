# Phase 5C — Pinned `elem.wast` Active-Element + Imported-Table Tranche

This tranche keeps the source-faithful supported subset of `WebAssembly/spec` `test/core/elem.wast` and extends the standard `spectest` harness binding to the already-supported imported `funcref` table surface.

## Provenance

- upstream repository: `WebAssembly/spec`
- pinned commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- upstream source: `test/core/elem.wast`
- committed fixture: `phase5c_upstream_elem_subset.wast`

## Selected semantics

Fifteen successful live modules cover the admitted active-element surface. The original seven defined-table modules remain unchanged and cover ordinary initialization, repeated offsets, executable `call_indirect` checks, the final valid slot, and zero-length exact-end boundaries.

Eight additional upstream module directives now cover the standard imported table forms already supported by the runtime:

- `(import "spectest" "table" (table 10 funcref))` with one active segment at offset 0
- the same ten-element import with repeated active segments at offsets 9, 3, 7, 3, and 5
- the final valid slot at offset 9 through an imported ten-element table
- an empty active segment at offset 0 through `(table 0 funcref)`
- non-empty active segments at offsets 0 and 1 through imports whose declared minima are zero
- declared import maxima of 100 and 30, both satisfied by the standard host table

The harness binds `spectest.table` as `TableHandle::new(10, Some(20))`, matching the standard spectest table limits. Each module directive still receives a fresh `HostRegistry` and therefore a fresh table handle. That isolation is intentional: these selected directives observe import compatibility and active initialization only; they do not claim cross-module funcref persistence, which remains outside the runtime's instance-bound `FunctionRef` model.

Eight upstream inline-module `assert_trap` directives continue to cover complementary instantiation bounds failures for defined tables, and one `assert_invalid` case continues to require a missing table to fail static validation specifically as `ValidationError::ElementTableOutOfBounds`.

## Phase-sensitive trap matching

The spec message `out of bounds table access` remains mapped to `RuntimeError::ElementSegmentOutOfBounds` for the selected inline-module instantiation assertions. Encoding, parsing, and static validation must succeed before instantiation may fail, so validation failures cannot be mistaken for runtime table-bound traps.

The Differential workspace continues to mirror an active-element OOB module against Wasmtime. The mini runtime's `ElementSegmentOutOfBounds` and Wasmtime's `TableOutOfBounds` normalize to the same table-out-of-bounds class at instantiation.

## Exact accounting

- `elem.wast`: 15 live modules / 11 executed assertions / 0 bare invokes / 0 filters
- full pinned manifest remains 33 unique sources / 997 executed assertions / 4 bare invokes / 0 filters
- this slice adds module coverage only; assertion and invoke totals do not change

## Scope

No runtime product behavior, dependency, MSRV, permanent workflow, warning policy, or CI acceptance rule changes. Reference-expression element encodings, `global.get`/extended-constant offsets, passive/declarative segments, bulk table instructions, binary-format variants, persistent cross-module spectest table identity, and unsupported validation messages remain outside this tranche rather than being filtered into apparent success.
