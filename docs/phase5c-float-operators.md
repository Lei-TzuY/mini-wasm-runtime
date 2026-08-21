# Phase 5C Float Operator Core

This slice completes the MVP no-immediate f32/f64 arithmetic operator family while keeping reinterpret and float/integer conversion instructions out of scope.

## Opcode coverage

The runtime, typed validator, and structured-control decoder agree on the complete float operator range:

- f32 unary `0x8b..=0x91`: `abs`, `neg`, `ceil`, `floor`, `trunc`, `nearest`, `sqrt`
- f32 binary `0x92..=0x98`: `add`, `sub`, `mul`, `div`, `min`, `max`, `copysign`
- f64 unary `0x99..=0x9f`: the corresponding seven unary operators
- f64 binary `0xa0..=0xa6`: the corresponding seven binary operators

Unary operators validate as `f32 -> f32` or `f64 -> f64`; binary operators validate as two equal-width operands to one equal-width result.

## Bit-exact sign operations

`abs`, `neg`, and `copysign` are implemented through raw IEEE-754 bit operations rather than host arithmetic. They alter only the sign bit, preserving all exponent and significand bits, including NaN payload bits.

## Rounding and signed zero

`ceil`, `floor`, and `trunc` use their directed rounding operation but explicitly preserve negative zero when a negative finite input rounds to zero.

`nearest` implements round-to-nearest, ties-to-even directly. Values already guaranteed integral at the f32/f64 precision boundary are returned unchanged. Negative inputs whose rounded result is zero return negative zero.

## NaN policy

For operators other than `abs`, `neg`, and `copysign`, the runtime chooses a positive canonical NaN whenever it must synthesize or normalize a NaN result. This is a valid WebAssembly result and avoids depending on host-specific NaN payload propagation.

`abs`, `neg`, and `copysign` are intentionally excluded from that normalization because WebAssembly defines them as sign manipulation over the original bit pattern.

## `sqrt`

`sqrt` preserves signed zero and positive infinity. Negative non-zero inputs, including negative infinity, produce canonical NaN. Ordinary non-negative finite values use the host square-root operation after those observable edge cases are handled.

## `min` / `max`

The runtime handles signed zero explicitly:

- `min(+0, -0) = -0`
- `max(+0, -0) = +0`
- equal-sign zero pairs preserve that sign

NaN inputs produce canonical NaN. Non-NaN, non-zero values use ordinary numeric ordering.

## Validation and control decoding

The typed validator assigns exact f32/f64 stack effects for every new opcode. The structured-control predecoder recognizes the entire contiguous no-immediate float range, so these operators work inside blocks, loops, and conditionals rather than only at top level.

## Adversarial coverage

Integration tests check raw bits for sign operations, directed-rounding signed zero, positive and negative halfway ties, large already-integral values, square-root edge cases, signed-zero min/max, NaN results, copysign payload preservation, existing arithmetic regression behavior, structured-control decoding, and validator type confusion.

## Deliberate non-goals

This slice does not add:

- trapping float-to-integer conversions
- saturating conversions
- integer-to-float conversions beyond the already supported subset
- reinterpret instructions
- multi-value execution
- SIMD or relaxed-SIMD numeric semantics
