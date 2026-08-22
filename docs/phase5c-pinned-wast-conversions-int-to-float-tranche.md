# Phase 5C — Pinned `conversions.wast` Integer-to-Float Tranche

This tranche extends the existing manifest row for the pinned `WebAssembly/spec` `test/core/conversions.wast` source with source-faithful integer-to-float conversion assertions. The runtime already implemented all eight integer-to-float conversion instructions; this work adds upstream WAST coverage without widening the supported WebAssembly surface.

## Provenance

- upstream repository: `WebAssembly/spec`
- pinned commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- upstream source: `test/core/conversions.wast`
- committed fixture: `phase5c_upstream_conversions_subset.wast`

The previous 48 width, trapping, and reinterpret assertions remain unchanged. This tranche adds sixteen assertions covering every integer-to-float conversion opcode.

## Semantic invariants

Integer-to-float conversion must:

- interpret `_s` inputs through their signed i32/i64 value
- interpret `_u` inputs through their unsigned i32/i64 bit pattern
- produce the correctly rounded IEEE-754 f32/f64 value
- use round-to-nearest, ties-to-even where the integer is not exactly representable
- never trap merely because the integer magnitude exceeds the exact-integer range of the destination float

The selected vectors intentionally cross both the destination precision boundaries and the signed/unsigned view boundary so a simplistic cast through the wrong intermediate type cannot pass accidentally.

## Selected upstream vectors

### Signed conversions to f32

`f32.convert_i32_s` covers the f32 integer precision boundary around `2^24`:

- `16777217 -> 16777216`
- `-16777219 -> -16777220`

`f32.convert_i64_s` covers both a full-width signed value and f32 rounding:

- `i64::MIN -> -9223372036854775808`
- `16777219 -> 16777220`

### Signed conversions to f64

`f64.convert_i32_s` covers exactly representable signed i32 values:

- `i32::MIN -> -2147483648`
- `987654321 -> 987654321`

`f64.convert_i64_s` covers the 64-bit signed domain and the f64 integer precision boundary around `2^53`:

- `i64::MIN -> -9223372036854775808`
- `9007199254740995 -> 9007199254740996`

### Unsigned conversions to f32

`f32.convert_i32_u` verifies that the source bit pattern is interpreted as u32 before conversion:

- `0xffffffff -> 4294967296`
- `16777219 -> 16777220`

`f32.convert_i64_u` does the same for u64 and also covers f32 rounding:

- `0xffffffffffffffff -> 18446744073709551616`
- `16777217 -> 16777216`

### Unsigned conversions to f64

`f64.convert_i32_u` verifies the unsigned view without any precision loss for the selected i32 values:

- `0x80000000 -> 2147483648`
- `0xffffffff -> 4294967295`

`f64.convert_i64_u` combines the full unsigned 64-bit range with the `2^53` precision boundary:

- `0xffffffffffffffff -> 18446744073709551616`
- `9007199254740995 -> 9007199254740996`

## Exact accounting

The single `conversions.wast` manifest row changes from:

- 1 module
- 48 executed assertions
- 0 filtered directives

to:

- 1 module
- 64 executed assertions
- 0 filtered directives

Across the existing twelve unique manifest source rows, selected pinned assertions increase from 156 to 172. The workspace remains at 256 top-level test functions because these vectors execute inside the existing manifest-driven regression.

## Validation boundary

The fixture/manifest-only semantic candidate passed the complete GitHub Actions matrix before this document was added:

- Rust stable / Ubuntu
- Rust stable / Windows
- Rust stable / macOS
- Rust 1.81.0 / Ubuntu

Ubuntu logs explicitly confirmed `phase5c_wast_ingestion` and `pinned_upstream_manifest_executes_with_exact_accounting` passing with the expanded 64-assertion conversions row. Final validation is rerun on the documentation-inclusive HEAD before the PR is sealed.

## Non-goals

This tranche does not change:

- parser, validator, interpreter, or host ABI behavior
- WAST runner filtering or trap mapping
- dependencies or CI policy
- float promotion/demotion coverage
- remaining trapping conversion families
- saturating conversion coverage

It does not claim complete `test/core/conversions.wast` support. Promotion/demotion and other already-supported conversion families remain separate reviewable manifest expansions.
