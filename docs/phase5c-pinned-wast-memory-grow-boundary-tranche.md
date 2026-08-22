# Phase 5C — Pinned `memory_grow.wast` Boundary Tranche

This tranche extends the existing pinned `WebAssembly/spec` `test/core/memory_grow.wast` fixture with the upstream memory-access-at-boundary module. It adds conformance evidence only; it does not widen parser, validator, runtime, host, or WAST-runner behavior.

## Provenance

- upstream repository: `WebAssembly/spec`
- pinned commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- upstream source: `test/core/memory_grow.wast`
- committed fixture: `phase5c_upstream_memory_grow_subset.wast`

## Selected boundary semantics

The added source-faithful module starts with zero pages and exports `memory.size`, `memory.grow`, and i32 loads/stores at address zero and one-page offset (`0x10000`). Its twenty directives prove the following sequence:

1. A zero-page memory reports size zero.
2. Loads and stores at both address zero and the one-page offset trap while no page exists.
3. Growing by one page returns the previous size, zero.
4. The newly allocated first page reads as zero.
5. A store at address zero persists.
6. The one-page offset remains out of bounds while memory has only one page.
7. Growing by four more pages returns previous size one and produces total size five.
8. Data written before growth remains intact after growth.
9. The former page-boundary address is now valid and initially zero-filled.
10. That newly valid address supports a normal store/load round trip.

This sequence catches several implementation errors that isolated tests can miss: returning the new rather than previous page count, incorrectly making the next-page boundary legal too early, failing to zero newly allocated pages, or reallocating growth in a way that loses existing bytes.

## Exact accounting

The existing `memory_grow.wast` row changes from:

- 1 module
- 8 executed assertions
- 0 filtered directives

to:

- 2 modules
- 28 executed assertions
- 0 filtered directives

Across the full pinned manifest, selected assertions increase from 214 to 234. The manifest continues to require zero filters for every row.

## Fail-closed boundary

This tranche does not change:

- WAST runner acceptance, filtering, trap mapping, or expected-value matching
- parser or validator behavior
- runtime/interpreter behavior
- host ABI or capability behavior
- Cargo dependencies
- workflow, warning, or CI policy

An OOB access unexpectedly succeeding, a valid access trapping, loss of pre-growth bytes, non-zero newly allocated memory, an incorrect `memory.grow` return value, or any selected directive becoming filtered causes the manifest regression to fail.

## Validation

The fixture/manifest-only semantic candidate passed the complete GitHub Actions matrix before this document was added:

- Rust stable / Ubuntu
- Rust stable / Windows
- Rust stable / macOS
- Rust 1.81.0 / Ubuntu

Ubuntu logs explicitly confirmed `phase5c_wast_ingestion` 3/3 and `pinned_upstream_manifest_executes_with_exact_accounting` passing with the expanded `memory_grow.wast` row. Final validation is rerun on the documentation-inclusive HEAD before the Draft PR is sealed.

## Non-goals

This is not a claim of complete upstream `memory_grow.wast` support. Control-context uses of `memory.grow` remain a separate reviewable candidate, especially cases that depend on unsupported instructions such as `select` or `br_table`.
