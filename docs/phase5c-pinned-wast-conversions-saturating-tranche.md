# Phase 5C — Pinned `conversions.wast` Saturating-Conversion Tranche

This tranche extends the existing pinned `WebAssembly/spec` `test/core/conversions.wast` fixture with source-faithful saturating float-to-integer assertions. The eight saturating conversion subopcodes were already implemented; this change adds upstream WAST coverage without widening the supported WebAssembly surface.

## Provenance

- upstream repository: `WebAssembly/spec`
- pinned commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- upstream source: `test/core/conversions.wast`
- committed fixture: `phase5c_upstream_conversions_subset.wast`

## Covered instructions

All eight `0xfc` saturating conversion subopcodes are represented:

- `i32.trunc_sat_f32_s`
- `i32.trunc_sat_f32_u`
- `i32.trunc_sat_f64_s`
- `i32.trunc_sat_f64_u`
- `i64.trunc_sat_f32_s`
- `i64.trunc_sat_f32_u`
- `i64.trunc_sat_f64_s`
- `i64.trunc_sat_f64_u`

## Selected semantics

Sixteen pinned assertions concentrate on the failure-prone saturation boundaries rather than duplicating ordinary finite-value unit coverage:

- representative signed NaNs produce zero rather than trapping
- positive infinity saturates to the signed maximum where selected
- negative infinity saturates to the signed minimum where selected
- unsigned positive infinity saturates to the all-ones u32/u64 bit pattern
- unsigned negative infinity saturates to zero
- both f32 and f64 source widths are exercised for both i32 and i64 destinations

These cases also exercise the `0xfc` prefixed-opcode decoder through the WAST encoder, validator, and runtime instead of invoking internal helpers directly.

## Exact accounting

The single `conversions.wast` manifest row changes from:

- 1 module
- 76 executed assertions
- 0 filtered directives

to:

- 1 module
- 92 executed assertions
- 0 filtered directives

Across the existing twelve unique source rows, the pinned manifest reaches exactly 200 selected assertions with zero filters. The workspace remains at 256 top-level test functions because these vectors execute inside the existing manifest regression.

## Validation boundary

The fixture/manifest-only semantic candidate passed the full GitHub Actions matrix before this document was added:

- Rust stable / Ubuntu
- Rust stable / Windows
- Rust stable / macOS
- Rust 1.81.0 / Ubuntu

Ubuntu logs explicitly confirmed `phase5c_wast_ingestion` 3/3 and `pinned_upstream_manifest_executes_with_exact_accounting` passing with the 92-assertion conversions row. Final validation is rerun on the documentation-inclusive HEAD before the PR is sealed.

## Non-goals

This tranche does not change parser, validator, interpreter, host ABI, WAST filtering, trap mapping, dependencies, workflows, warning policy, or CI policy. It does not claim complete upstream `conversions.wast` coverage; the selected vectors are a reviewable supported subset pinned to the recorded upstream commit.
