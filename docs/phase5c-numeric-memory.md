# Phase 5C — Typed Numeric Memory

This focused Phase-5C slice completes the MVP numeric load/store families for the runtime's existing four numeric value types while preserving the same typed-validator and checked-linear-memory boundaries used by the earlier i32 memory subset.

## Supported opcodes

### Loads

| Opcode | Instruction | Address | Result | Natural alignment |
| --- | --- | --- | --- | --- |
| `0x28` | `i32.load` | i32 | i32 | 2 |
| `0x29` | `i64.load` | i32 | i64 | 3 |
| `0x2a` | `f32.load` | i32 | f32 | 2 |
| `0x2b` | `f64.load` | i32 | f64 | 3 |
| `0x2c` | `i32.load8_s` | i32 | i32 | 0 |
| `0x2d` | `i32.load8_u` | i32 | i32 | 0 |
| `0x2e` | `i32.load16_s` | i32 | i32 | 1 |
| `0x2f` | `i32.load16_u` | i32 | i32 | 1 |
| `0x30` | `i64.load8_s` | i32 | i64 | 0 |
| `0x31` | `i64.load8_u` | i32 | i64 | 0 |
| `0x32` | `i64.load16_s` | i32 | i64 | 1 |
| `0x33` | `i64.load16_u` | i32 | i64 | 1 |
| `0x34` | `i64.load32_s` | i32 | i64 | 2 |
| `0x35` | `i64.load32_u` | i32 | i64 | 2 |

### Stores

| Opcode | Instruction | Address | Value | Natural alignment |
| --- | --- | --- | --- | --- |
| `0x36` | `i32.store` | i32 | i32 | 2 |
| `0x37` | `i64.store` | i32 | i64 | 3 |
| `0x38` | `f32.store` | i32 | f32 | 2 |
| `0x39` | `f64.store` | i32 | f64 | 3 |
| `0x3a` | `i32.store8` | i32 | i32 | 0 |
| `0x3b` | `i32.store16` | i32 | i32 | 1 |
| `0x3c` | `i64.store8` | i32 | i64 | 0 |
| `0x3d` | `i64.store16` | i32 | i64 | 1 |
| `0x3e` | `i64.store32` | i32 | i64 | 2 |

Alignment immediates remain hints bounded by each instruction's natural alignment exponent; over-aligned encodings are rejected statically.

## Runtime semantics

All memory instructions continue to use the same 32-bit effective-address model:

```text
effective = zero_extend_u32(address) + zero_extend_u32(offset)
```

The full byte range is checked before any read or write. Out-of-bounds accesses return the existing precise `MemoryOutOfBounds { address, width }` runtime error.

Integer values use little-endian byte order. Narrow i64 stores truncate to the low 8, 16, or 32 bits. Signed narrow loads sign-extend; unsigned narrow loads zero-extend.

Floating-point memory operations are bit-preserving transfers, not numeric conversions:

- `f32.store` writes `value.to_bits().to_le_bytes()`;
- `f32.load` reconstructs with `f32::from_bits`;
- `f64.store` and `f64.load` use the corresponding u64 bit representation.

This preserves NaN payload bits across memory round trips. Arithmetic NaN propagation remains a separate numeric-semantics concern.

## Owned and imported memory

The interpreter routes every load/store through the same `with_memory` / `with_memory_mut` abstraction. Therefore the new instructions behave identically for:

- instance-owned `LinearMemory`;
- imported shared `MemoryHandle` backing.

The imported-memory path is explicitly tested with a non-canonical f64 NaN payload and raw host-side byte inspection, so little-endian layout and payload preservation are externally observable rather than inferred from a store-then-load pair.

## Validator invariants

The typed validator applies exact stack effects:

- every memory address is i32;
- each load pushes the opcode's declared numeric result type;
- each store consumes its declared numeric value type and then its i32 address;
- all supported load/store opcodes validate their own natural-alignment ceiling;
- memory instructions still require the current single memory index space to exist.

Runtime type checks remain in place behind validation as defense in depth.

## Adversarial coverage

The focused integration suite covers:

- full-width i64 store/load execution;
- i64 8/16/32-bit truncating stores;
- signed and unsigned extension for every narrow i64 load width;
- f32 NaN payload preservation;
- shared imported f64 NaN payload preservation plus raw little-endian byte layout;
- wrong-type numeric store rejection by the validator;
- over-alignment rejection for every newly supported load/store opcode;
- exact OOB widths for new full-width load/store operations.

## Non-goals

This slice does not add SIMD, memory64, multiple memories, bulk-memory instructions, reinterpret instructions, trapping float-to-integer conversions, multi-value execution, or host ABI changes.
