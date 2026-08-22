# Phase 5C pinned `conversions.wast` promotion/demotion tranche

This tranche extends the existing pinned `WebAssembly/spec` `test/core/conversions.wast` fixture with already-supported floating-point width conversion semantics.

## Provenance

- upstream repository: `WebAssembly/spec`
- pinned commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- upstream source: `test/core/conversions.wast`
- local fixture: `crates/wasm-runtime/tests/fixtures/phase5c_upstream_conversions_subset.wast`

The existing manifest row remains unique for this upstream source and changes from `1 / 64 / 0` to `1 / 76 / 0` for modules / executed assertions / filters.

## Selected semantics

The twelve added source-faithful assertions cover both width-conversion opcodes:

- `f64.promote_f32`
  - negative-zero sign preservation
  - exact promotion of the minimum positive f32 subnormal
  - exact promotion of the maximum finite f32 value
  - positive infinity preservation
- `f32.demote_f64`
  - negative-zero sign preservation
  - positive minimum-f64-subnormal underflow to `+0`
  - negative minimum-f64-subnormal underflow to `-0`
  - selected subnormal-boundary rounding
  - selected finite upper-boundary rounding
  - round-to-nearest, ties-to-even near `1.0`
  - overflow to positive infinity
  - overflow to negative infinity

Promotion from f32 to f64 is exact for finite f32 values. Demotion from f64 to f32 is a narrowing IEEE-754 conversion and therefore deliberately exercises signed underflow, rounding, and overflow. These are numeric conversions, not bit reinterpretations.

## Fail-closed contract

No WAST runner, filter, NaN matcher, trap mapper, parser, validator, interpreter, host ABI, dependency, workflow, warning policy, or CI acceptance rule is changed. The manifest continues to require exact accounting; a selected assertion becoming filtered or changing its result makes the regression fail.

## Non-goals

This tranche does not claim complete `conversions.wast` coverage. Saturating float-to-integer conversions remain a separate candidate so their clamp/NaN semantics can be reviewed independently from IEEE floating-point width conversion.
