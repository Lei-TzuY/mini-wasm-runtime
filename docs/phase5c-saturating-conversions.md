# Phase 5C Saturating Numeric Conversions

This slice adds the eight saturating float-to-integer conversion instructions encoded under the `0xfc` numeric prefix. It also introduces a reusable prefixed-opcode decode/error path for the runtime and validator.

## Binary decoding

`0xfc` is an opcode prefix, not a complete instruction by itself. It is followed by a u32 LEB128 subopcode. The supported subopcodes in this slice are:

- 0: `i32.trunc_sat_f32_s`
- 1: `i32.trunc_sat_f32_u`
- 2: `i32.trunc_sat_f64_s`
- 3: `i32.trunc_sat_f64_u`
- 4: `i64.trunc_sat_f32_s`
- 5: `i64.trunc_sat_f32_u`
- 6: `i64.trunc_sat_f64_s`
- 7: `i64.trunc_sat_f64_u`

The interpreter, typed validator, and structured-control predecoder all use the existing u32 LEB decoder for the subopcode. Unsupported subopcodes retain both the prefix and the fully decoded u32 value in `UnsupportedPrefixedOpcode`, so a multi-byte subopcode such as 128 is not collapsed to a single byte or misreported as plain `0xfc`.

Malformed subopcode LEB128 is rejected through the existing decode/malformed-immediate path.

## Saturating semantics

Unlike the trapping `trunc` instructions, `trunc_sat` never traps because of the floating payload.

For every destination width and signedness:

1. NaN produces zero.
2. The finite/infinite value is conceptually truncated toward zero.
3. Results below the destination range clamp to the minimum.
4. Results above the destination range clamp to the maximum.
5. In-range finite results are converted normally.

For signed iN, the mathematical range is `[-2^(N-1), 2^(N-1)-1]` after saturation. For unsigned iN, negative values clamp to zero and values at or above `2^N` clamp to `2^N-1`.

The runtime stores unsigned maxima using the WebAssembly integer bit pattern, so u32 max is represented as `Value::I32(-1)` and u64 max as `Value::I64(-1)`.

## NaN and infinity

NaN is checked before numeric comparison and always returns zero.

Infinity then follows the ordinary saturation comparisons:

- signed negative infinity -> MIN
- signed positive infinity -> MAX
- unsigned negative infinity -> 0
- unsigned positive infinity -> all-ones maximum

No `InvalidConversionToInteger` or `IntegerOverflow` error is produced by a supported saturating instruction.

## Validator

The typed validator decodes the u32 subopcode and enforces:

- subopcode 0/1: `f32 -> i32`
- subopcode 2/3: `f64 -> i32`
- subopcode 4/5: `f32 -> i64`
- subopcode 6/7: `f64 -> i64`

Unsupported subopcodes fail before execution with a structured prefixed-opcode validation error.

## Structured-control decoder

The control-map predecoder recognizes `0xfc`, consumes exactly one u32 LEB subopcode, and accepts only 0 through 7 in this slice. This keeps instruction boundaries correct when saturating conversions appear inside blocks, loops, or conditionals and establishes the decoder shape needed by future `0xfc` instruction families.

## Adversarial coverage

Integration tests cover:

- NaN for all eight subopcodes;
- positive and negative infinities;
- finite signed and unsigned overflow for i32/u32/i64/u64 targets;
- in-range truncation toward zero;
- negative unsigned values, including fractions;
- execution inside structured control;
- typed-validator source-type confusion;
- unsupported subopcodes 8 and multi-byte 128;
- malformed u32 LEB subopcode encoding.

## Deliberate non-goals

This slice does not implement the other `0xfc` instruction families such as bulk-memory or table operations. Unsupported subopcodes remain fail-closed until their complete parser/validator/runtime semantics are implemented.
