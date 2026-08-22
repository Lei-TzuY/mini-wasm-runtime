# Phase 6 structured generated properties and shrinking

This tranche extends deterministic property testing from flat numeric pairs into generated program structure while keeping normal CI fully reproducible and dependency-free.

The committed generator builds bounded i32 expression trees from four function parameters and six wrapping/bitwise operators. Every generated tree is compiled to WebAssembly instructions, passed through the public parser/validator/runtime path, and compared against an independent Rust reference evaluator. The same tree generator is embedded inside typed `if (result i32)` modules so structured-control selection and generated expression semantics are checked together.

A second generated domain probes i32 memory store/load round trips across several memarg offsets and addresses biased around the 64 KiB page boundary. In-bounds effective addresses must return the exact stored value; out-of-bounds effective addresses must trap specifically as a four-byte memory access.

## Deterministic shrinking

Expression failures use a greedy structural shrinker. It tries replacing a binary expression by either child, then recursively simplifies children and parameter leaves while accepting only candidates that still reproduce the failure. Failure diagnostics preserve the fixed seed and case index and report both the original and minimized trees. A dedicated test verifies that the shrinker reduces a synthetic nested counterexample to its minimal operation-bearing form.

The shrinker is intentionally small and deterministic rather than a general-purpose property framework. This keeps the Rust 1.81 MSRV and dependency surface unchanged while making generated failures much easier to replay and diagnose.

Future work can expand structured generation to multi-value control, tables/indirect calls, imported state, and richer memory sequences. Those domains should retain bounded generation, explicit reference semantics, deterministic replay, and minimized regression fixtures.
