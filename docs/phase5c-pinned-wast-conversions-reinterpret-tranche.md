# Phase 5C — Pinned `conversions.wast` Reinterpret Tranche

This tranche extends the existing manifest row for the pinned `WebAssembly/spec` `test/core/conversions.wast` source with source-faithful reinterpret assertions. The runtime already implemented all four reinterpret instructions; this work adds upstream WAST conformance coverage without widening the supported WebAssembly surface.

## Provenance

- upstream repository: `WebAssembly/spec`
- pinned commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- upstream source: `test/core/conversions.wast`
- committed fixture: `phase5c_upstream_conversions_subset.wast`

The previous 36 width/trapping conversion assertions remain unchanged. This tranche adds twelve assertions, three for each reinterpret direction.

## Bit-exact invariant

The four reinterpret instructions change only the type through which a fixed-width bit pattern is viewed:

- `f32.reinterpret_i32`
- `f64.reinterpret_i64`
- `i32.reinterpret_f32`
- `i64.reinterpret_f64`

They are not numeric conversions. They must not round, canonicalize NaNs, normalize signed zero, saturate, trap, or otherwise modify the source bits.

## Selected upstream vectors

### Integer bits to float

`f32.reinterpret_i32` covers:

- `0x80000000 -> -0.0`
- `1 ->` the minimum positive f32 subnormal
- `0x7fa00000 -> nan:0x200000`

`f64.reinterpret_i64` covers:

- `0x8000000000000000 -> -0.0`
- `1 ->` the minimum positive f64 subnormal
- `0x7ff4000000000000 -> nan:0x4000000000000`

These vectors detect sign-bit loss, subnormal conversion, and NaN-payload normalization.

### Float bits to integer

`i32.reinterpret_f32` covers:

- `-0.0 -> 0x80000000`
- `-nan:0x7fffff -> -1`, the all-ones 32-bit pattern
- `nan:0x200000 -> 0x7fa00000`

`i64.reinterpret_f64` covers:

- `-0.0 -> 0x8000000000000000`
- `-nan:0xfffffffffffff -> -1`, the all-ones 64-bit pattern
- `nan:0x4000000000000 -> 0x7ff4000000000000`

The all-ones and explicit-payload cases are especially useful adversarial checks: a floating-point arithmetic path or NaN canonicalization step cannot satisfy them accidentally.

## Exact accounting

The single `conversions.wast` manifest row changes from:

- 1 module
- 36 executed assertions
- 0 filtered directives

to:

- 1 module
- 48 executed assertions
- 0 filtered directives

Across the existing twelve unique manifest source rows, selected pinned assertions increase from 144 to 156. The workspace remains at 256 top-level test functions because the new vectors execute inside the existing manifest-driven regression.

## Validation boundary

The fixture/manifest-only semantic candidate passed the complete GitHub Actions matrix before this document was added:

- Rust stable / Ubuntu
- Rust stable / Windows
- Rust stable / macOS
- Rust 1.81.0 / Ubuntu

Ubuntu logs explicitly confirmed `phase5c_wast_ingestion` and `pinned_upstream_manifest_executes_with_exact_accounting` passing with the expanded conversions row. Final validation is rerun on the documentation-inclusive HEAD before the PR is sealed.

## Non-goals

This tranche does not change:

- parser, validator, interpreter, or host ABI behavior
- WAST runner filtering or trap mapping
- dependencies or CI policy
- integer-to-float conversion coverage
- float promotion/demotion coverage
- remaining trapping or saturating conversion coverage

It does not claim complete `test/core/conversions.wast` support. Those already-supported families remain separate reviewable manifest expansions.
