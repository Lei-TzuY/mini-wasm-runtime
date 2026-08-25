# Phase 6 structured generated properties and shrinking

This tranche extends deterministic property testing from flat numeric pairs into generated program structure and persistent runtime state while keeping normal CI fully reproducible and dependency-free.

## Existing structured domains

The original generator builds bounded i32 expression trees from four function parameters and six wrapping/bitwise operators. Every generated tree is compiled to WebAssembly instructions, passed through the public parser/validator/runtime path, and compared against an independent Rust reference evaluator. The same tree generator is embedded inside typed `if (result i32)` modules so structured-control selection and generated expression semantics are checked together.

A second generated domain probes i32 memory store/load round trips across several memarg offsets and addresses biased around the 64 KiB page boundary. In-bounds effective addresses must return the exact stored value; out-of-bounds effective addresses must trap specifically as a four-byte memory access.

## Broader structured and stateful domains

`phase6_structured_stateful_properties.rs` broadens the deterministic generator surface across the remaining roadmap targets:

- multi-value `if` modules generate independent `[i32, i64]` result pairs for both arms and verify exact result ordering through `invoke_export_values`
- generated `funcref` tables dispatch between two independently parameterized targets while also exercising null-element and out-of-bounds trap classes
- a mutable imported i32 global is driven through a long guest-update / host-override sequence and compared after every step with an independent wrapping-state model
- shared imported memory is driven through a persistent sequence of guest stores, host writes, and guest loads across fixed boundary-biased and generated aligned addresses; a host-side shadow model checks both directions of aliasing after every step

The generators use fixed seeds, bounded case counts, explicit reference state, and seed/case-or-step diagnostics. They do not depend on Wasmtime, proptest, quickcheck, or another property framework, so the normal workspace remains compatible with the declared Rust 1.81 MSRV and deterministic on every CI platform.

## Deterministic shrinking

Expression failures use a greedy structural shrinker. It tries replacing a binary expression by either child, then recursively simplifies children and parameter leaves while accepting only candidates that still reproduce the failure. Failure diagnostics preserve the fixed seed and case index and report both the original and minimized trees. A dedicated test verifies that the shrinker reduces a synthetic nested counterexample to its minimal operation-bearing form.

The shrinker is intentionally small and deterministic rather than a general-purpose property framework. Stateful multi-value/table/import/memory domains currently preserve exact seed and step replay instead of pretending to provide a generic shrinker for state-machine traces. Future hardening can add domain-specific trace reduction when one of these generators exposes a real regression.
