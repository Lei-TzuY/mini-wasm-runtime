# Phase 5C — MVP Integer Operator Core

This slice completes the MVP i32/i64 integer operator families around the arithmetic already present in the runtime. The implementation keeps integer semantics centralized in the numeric layer while the validator and control-map decoder carry matching opcode coverage.

## Supported operators

### i32

- `clz`, `ctz`, `popcnt`
- `add`, `sub`, `mul`
- `div_s`, `div_u`, `rem_s`, `rem_u`
- `and`, `or`, `xor`
- `shl`, `shr_s`, `shr_u`
- `rotl`, `rotr`

These occupy MVP opcodes `0x67..=0x78`.

### i64

- `clz`, `ctz`, `popcnt`
- `add`, `sub`, `mul`
- `div_s`, `div_u`, `rem_s`, `rem_u`
- `and`, `or`, `xor`
- `shl`, `shr_s`, `shr_u`
- `rotl`, `rotr`

These occupy MVP opcodes `0x79..=0x8a`.

## Trap semantics

Integer traps are explicit runtime outcomes rather than Rust arithmetic accidents.

### Division/remainder by zero

Every signed/unsigned division and remainder operation checks the divisor first. A zero divisor returns:

```text
RuntimeError::IntegerDivisionByZero
```

### Signed division overflow

WebAssembly traps only on the signed division overflow case:

```text
i32::MIN / -1
i64::MIN / -1
```

The runtime reports:

```text
RuntimeError::IntegerOverflow
```

Signed remainder is different: `MIN % -1` produces zero and must not trap. The evaluator handles this edge explicitly before using Rust `%`.

## Unsigned interpretation

Unsigned division/remainder reinterprets the same bit pattern as `u32`/`u64`, performs the unsigned operation, then returns the result through the corresponding signed `Value::I32`/`Value::I64` storage variant. No numeric conversion is implied.

## Shift and rotate counts

WebAssembly masks integer shift/rotate counts:

- i32 uses the low 5 bits (modulo 32);
- i64 uses the low 6 bits (modulo 64).

`shr_s` preserves signed arithmetic-shift semantics. `shr_u` reinterprets the left operand as unsigned before shifting.

## Validator invariants

The typed validator models:

- i32 count operators as `i32 -> i32`;
- i32 binary operators as `i32, i32 -> i32`;
- i64 count operators as `i64 -> i64`;
- i64 binary operators as `i64, i64 -> i64`.

A mismatched numeric width is rejected statically. Runtime typed pops remain as defense in depth.

## Control-map decoding

All opcodes `0x67..=0x8a` are no-immediate instructions. The structured-control predecoder recognizes the complete range so these operators execute correctly inside `block`, `loop`, and `if` bodies instead of being rejected during control-map construction.

## Adversarial coverage

Tests cover:

- i32/i64 `clz`, `ctz`, `popcnt`;
- signed truncation-toward-zero division/remainder;
- unsigned operations on high-bit-set operands;
- zero traps for every div/rem family;
- signed `MIN / -1` overflow for both widths;
- non-trapping signed `MIN % -1`;
- bitwise and/or/xor;
- signed vs unsigned right shift;
- shift-count masking beyond the natural width;
- rotate-count masking;
- execution inside structured control;
- validator rejection of i64 operands supplied to an i32 operator.

## Non-goals

This slice does not add:

- floating unary operators (`abs`, `neg`, `ceil`, `floor`, `trunc`, `nearest`, `sqrt`);
- floating `min`, `max`, `copysign`;
- float-to-integer trapping conversions;
- integer-to-float conversions beyond the already supported subset;
- reinterpret instructions;
- saturating conversions;
- SIMD numeric operators.
