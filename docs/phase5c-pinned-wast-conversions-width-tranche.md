# Phase 5C — Pinned `conversions.wast` Integer-Width Tranche

This tranche extends the existing manifest row for the pinned `WebAssembly/spec` `test/core/conversions.wast` source. It adds source-faithful assertions for integer width/sign-view conversions that were already implemented by the runtime; it does not widen the supported WebAssembly surface.

## Provenance

- upstream repository: `WebAssembly/spec`
- pinned commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- upstream source: `test/core/conversions.wast`
- committed fixture: `phase5c_upstream_conversions_subset.wast`

The existing f32-to-i32 and f64-to-i64 trapping conversion assertions remain unchanged. This tranche adds twelve selected assertions from the same pinned source.

## Selected semantics

### `i64.extend_i32_s`

The selected vectors lock sign extension for:

- zero
- `-1`
- `i32::MAX`
- the high-bit-set `0x80000000` pattern, which must become `0xffffffff80000000`

### `i64.extend_i32_u`

The selected vectors lock zero extension of the original 32-bit pattern for:

- a negative decimal input (`-10000`)
- `-1`, yielding `0x00000000ffffffff`
- `i32::MAX`
- `0x80000000`, yielding `0x0000000080000000`

The signed Rust representation of the input is therefore only a host view; the WebAssembly unsigned conversion operates on the source bit pattern.

### `i32.wrap_i64`

The selected vectors lock modulo-`2^32` truncation for:

- `-1`
- an i64 with a zero low word (`0xffffffff00000000`)
- the mixed pattern `0x123456789abcdef0`, whose low word is `0x9abcdef0`
- `0x0000000100000001`, whose low word is `1`

No overflow trap is permitted for `i32.wrap_i64`; discarded high bits do not influence the result.

## Exact accounting

The single `conversions.wast` manifest row changes from:

- 1 module
- 24 executed assertions
- 0 filtered directives

to:

- 1 module
- 36 executed assertions
- 0 filtered directives

Across the existing twelve unique manifest source rows, the selected pinned assertion count therefore increases from 132 to 144. The workspace remains at 256 top-level test functions because these vectors execute inside the existing manifest-driven regression.

The manifest runner continues to require exact pinned provenance, unique source/fixture mappings, and exact module/executed/filter counts. A selected assertion becoming filtered or changing result semantics is a failure.

## Validation boundary

The candidate containing only the fixture and manifest accounting passed the full GitHub Actions matrix before this document was added:

- Rust stable / Ubuntu
- Rust stable / Windows
- Rust stable / macOS
- Rust 1.81.0 / Ubuntu

Every environment passed formatting, Clippy with warnings denied, the complete workspace test suite, and documentation generation. Final validation is rerun on the documentation-inclusive HEAD before the PR is sealed.

## Non-goals

This tranche does not add or change:

- parser, validator, interpreter, or host ABI behavior
- WAST filtering or trap mapping
- dependencies or CI policy
- reinterpret instructions in the pinned WAST manifest
- integer-to-float conversions
- float promotion/demotion
- the remaining trapping or saturating conversion families

Those already-supported families remain candidates for separate, reviewable manifest expansions. This tranche is not a claim of complete `conversions.wast` support.
