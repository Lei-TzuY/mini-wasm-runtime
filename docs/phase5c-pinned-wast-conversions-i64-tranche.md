# Phase 5C — Pinned `conversions.wast` i64 Tranche

This tranche extends the existing manifest row for `WebAssembly/spec` `test/core/conversions.wast` rather than creating a duplicate provenance row.

## Provenance

- upstream repository: `WebAssembly/spec`
- pinned commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- upstream source: `test/core/conversions.wast`
- committed fixture: `phase5c_upstream_conversions_subset.wast`

The existing f32-to-i32 trapping conversion assertions remain unchanged. This tranche adds selected f64-to-i64 signed and unsigned assertions from the same pinned source.

## Selected semantics

`i64.trunc_f64_s` coverage locks:

- truncation toward zero for positive and negative fractions
- the largest selected in-range positive f64 integer below `i64::MAX + 1`
- exact acceptance of `i64::MIN`
- positive signed overflow trapping as `integer overflow`
- NaN trapping as `invalid conversion to integer`

`i64.trunc_f64_u` coverage locks:

- truncation toward zero
- the `2^63` result preserving its unsigned bit pattern in `Value::I64`
- the largest selected representable f64 below `2^64`
- negative fractions in `(-1, 0)` truncating to zero
- `2^64` and `-1` trapping as `integer overflow`

## Exact accounting

The single `conversions.wast` manifest row changes from 12 to 24 executed assertions. It remains one module and zero filtered directives. All other manifest rows remain unchanged.

The manifest runner already requires exact module/executed/filter counts and exact pinned provenance. A selected assertion becoming filtered or changing trap/result behavior therefore fails the test.

## Scope

This is tests/docs only. It does not change parser, validator, interpreter, host ABI, trap mapping, filter policy, dependencies, or CI policy.

This tranche is not a claim of complete `conversions.wast` coverage. Other already-supported conversion families, including f32-to-i64, f64-to-i32, integer-to-float, reinterpret, promotion/demotion, and saturating conversions, remain candidates for later reviewable expansions.
