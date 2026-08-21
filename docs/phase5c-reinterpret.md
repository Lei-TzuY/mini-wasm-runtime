# Phase 5C Numeric Reinterpret

This slice adds the four MVP equal-width reinterpret instructions and nothing else:

- `i32.reinterpret_f32` (`0xbc`)
- `i64.reinterpret_f64` (`0xbd`)
- `f32.reinterpret_i32` (`0xbe`)
- `f64.reinterpret_i64` (`0xbf`)

## Core invariant

Reinterpretation preserves the complete underlying bit sequence. It does not perform arithmetic conversion and therefore must not round, canonicalize NaNs, change a sign bit, saturate, or trap because of the payload value.

The runtime implements float-to-integer reinterpretation with `to_bits()` followed by an equal-width integer view, and integer-to-float reinterpretation with `from_bits()` over the equal-width unsigned bit pattern.

This deliberately differs from numeric casts such as `value as i32` or integer-to-float conversion, which change the represented numerical value.

## Observable edge cases

Bit preservation includes:

- positive and negative zero
- infinities
- canonical and non-canonical NaN payloads
- signaling-looking NaN bit patterns
- integers with the high/sign bit set
- all-ones bit patterns

No special-case path exists for any of them.

## Validator

The typed validator enforces the exact source and target stack types:

- `f32 -> i32`
- `f64 -> i64`
- `i32 -> f32`
- `i64 -> f64`

A value of the wrong source type is rejected before execution.

## Structured-control decoder

The runtime control-map predecoder recognizes `0xbc..=0xbf` as no-immediate instructions. Reinterpret operations therefore remain valid inside blocks, loops, and conditionals rather than only in top-level instruction streams.

## Adversarial coverage

Integration tests compare raw bit patterns rather than floating-point equality. Coverage includes both directions at both widths, NaN payload round-trips, negative-zero round-trips, high-bit/all-ones values, structured-control execution, and validator type confusion.

## Deliberate non-goals

This slice does not add:

- float-to-integer trapping conversions
- saturating conversions
- additional integer-to-float conversions
- multi-value execution
- SIMD reinterpretation or vector lane semantics
