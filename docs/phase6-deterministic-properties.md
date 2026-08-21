# Phase 6 — deterministic property and metamorphic corpus

This slice adds generated-property coverage for semantics that are already implemented. It is intentionally deterministic: the test corpus uses a fixed non-zero xorshift64 seed and never depends on wall-clock time, process state, platform entropy, or an external random-number crate.

The purpose is broader semantic coverage without turning normal CI into fuzzing. Every generated case is reproducible from the committed seed and iteration index, and assertion messages include the relevant operands or raw bit patterns.

## Properties

The initial corpus exercises five property families through the public parse / instantiate / invoke path.

### i32 wrapping arithmetic

`i32.add`, `i32.sub`, and `i32.mul` are checked against Rust's explicit wrapping operations. The corpus includes fixed boundary pairs plus 512 generated pairs. Overflow is expected to wrap exactly modulo 2^32; a debug-build panic or saturation would violate the property.

### signed division and remainder

Generated non-trapping `i32.div_s` / `i32.rem_s` pairs are checked against truncation-toward-zero reference semantics. The corpus excludes only the two WebAssembly trap domains: divisor zero and `i32::MIN / -1`.

For every accepted pair it also checks:

- `q * b + r == a` in two's-complement arithmetic;
- a non-zero remainder has the dividend's sign;
- `abs(r) < abs(b)`.

These relational assertions are useful because they can catch a shared implementation bug that still happens to match one direct expected value.

### i64 shift and rotate counts

`i64.shl`, `i64.shr_s`, `i64.shr_u`, `i64.rotl`, and `i64.rotr` are checked over 512 generated `(value, count)` pairs. Reference operations use wrapping shifts / rotates so the effective count is reduced modulo 64, including negative and very large bit-pattern counts.

### reinterpret round trips

Generated i32 and i64 source bit patterns are sent through:

- `i32 -> f32 -> i32` reinterpret;
- `i64 -> f64 -> i64` reinterpret.

All 512 generated patterns in each width must return exactly unchanged. This covers arbitrary NaN payloads, signed zero, infinities, subnormals, and ordinary finite encodings without classifying or normalizing them.

### numeric memory round trips

Generated i64 values and arbitrary f32/f64 bit patterns are stored to linear memory and loaded back. Float loads are immediately reinterpreted to integers before comparison, so NaN equality rules cannot hide payload changes.

The property therefore checks the entire store/load byte path, alignment immediates, and bit preservation rather than only arithmetic value equality.

## Determinism and failure replay

The generator seed is committed in the test source. Each property derives a distinct deterministic stream from that seed. Re-running the same commit executes the same generated sequence on every CI platform.

This is deliberate. Fuzzing belongs to a separate Phase 6 surface because fuzzing optimizes exploration and crash discovery, while the normal test suite must remain stable and replayable.

## Dependency policy

No property-testing dependency is required for this initial corpus. The hand-sized deterministic generator keeps the MSRV and dependency surface unchanged while still exercising generated domains.

A future property framework may be justified if shrinking, richer structured generators, or reusable strategy composition materially improve defect diagnosis. Adding such a framework is not required to preserve the current properties.

## Non-goals

This slice does not claim:

- exhaustive input coverage;
- coverage-guided fuzzing;
- automatic shrinking of failing generated inputs;
- differential execution against a reference WebAssembly engine;
- upstream `.wast` spec-test ingestion;
- performance benchmarking.

Those remain separate hardening surfaces so each can have an explicit acceptance boundary.
