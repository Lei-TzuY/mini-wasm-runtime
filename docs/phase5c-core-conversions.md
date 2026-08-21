# Phase 5C Core Numeric Conversions

This slice completes the unprefixed MVP conversion opcode range `0xa7..=0xbf`. Saturating conversions under the `0xfc` prefix remain deliberately separate because they require prefixed-opcode decoding and have different non-trapping semantics.

## Trapping float-to-integer conversions

Newly supported instructions:

- `i32.trunc_f32_s` / `i32.trunc_f32_u`
- `i32.trunc_f64_s` / `i32.trunc_f64_u`
- `i64.trunc_f32_s` / `i64.trunc_f32_u`
- `i64.trunc_f64_s` / `i64.trunc_f64_u`

The runtime follows the WebAssembly operation order explicitly:

1. reject NaN as `InvalidConversionToInteger`;
2. reject infinities as `IntegerOverflow`;
3. truncate the finite value toward zero;
4. check the truncated mathematical integer against the destination range;
5. only then perform the host-language integer cast.

For signed iN, the truncated value must lie in `[-2^(N-1), 2^(N-1))`. For unsigned iN, it must lie in `[0, 2^N)`.

Because the range check occurs after truncation, finite values in `(-1, 0)` are valid unsigned inputs and produce zero. `-1.0` is out of range and traps.

The implementation intentionally compares against exact power-of-two bounds instead of values such as `i64::MAX as f64`, whose host conversion would round the boundary before validation.

## Integer-to-float conversions

Newly supported instructions:

- `f32.convert_i32_s` / `f32.convert_i32_u`
- `f32.convert_i64_s` / `f32.convert_i64_u`
- `f64.convert_i32_s` / `f64.convert_i32_u`
- `f64.convert_i64_s` / `f64.convert_i64_u`

Unsigned forms reinterpret the integer operand as u32/u64 before floating conversion; they do not treat a high-bit-set WebAssembly integer as a negative mathematical input.

These conversions are non-trapping. Tests cover representability boundaries at `2^24` for f32 and `2^53` for f64 to verify round-to-nearest, ties-to-even behavior.

## Unified conversion dispatch

With this slice, every unprefixed numeric conversion opcode from `0xa7` through `0xbf` is implemented. Runtime dispatch and the structured-control predecoder therefore use a single contiguous `0xa7..=0xbf` range, while the typed validator still records each instruction's exact source and destination types.

## Trap model

`RuntimeError::IntegerOverflow` is shared by signed division overflow and numeric conversion overflow, so its display text is generalized to `integer overflow`.

`RuntimeError::InvalidConversionToInteger` distinguishes NaN conversion traps from range overflow. The WebAssembly specification requires both situations to trap; retaining the distinction is useful to embedders and tests.

## Adversarial coverage

Integration tests cover:

- signed truncation toward zero for i32/i64 targets;
- unsigned values between -1 and 0;
- NaNs across all eight trapping opcodes;
- positive and negative infinities;
- exact f32/f64 source boundaries for i32/u32 targets;
- representable f64 boundaries around i64/u64 limits;
- all eight signed/unsigned integer-to-float opcodes;
- ties-to-even integer-to-float rounding;
- structured-control decoding;
- typed-validator source-type confusion.

## Deliberate non-goals

This slice does not add:

- `0xfc` saturating float-to-integer conversions;
- multi-value execution;
- SIMD conversion instructions;
- relaxed-SIMD conversion semantics.
