# Phase 6 reference differential tranche

The repository has an isolated reference-engine test workspace under `differential/`. It compiles supported WebAssembly modules from WAT, executes the exact same bytes in the mini runtime and Wasmtime, normalizes observable outcomes, and compares both engines against explicit expectations.

The fixed corpus covers integer wrapping and rotation, `nop`/`drop`/untyped `select`, indexed/default `br_table`, f32/f64 bit-level edge behavior, typed memory round trips, `memory.grow`, and representative execution traps.

The differential harness compares exact normalized trap classes shared by both engines. The original classes cover memory out-of-bounds, signed integer division overflow, integer division by zero, and invalid float-to-integer conversion. The table tranche adds table out-of-bounds, indirect call to an uninitialized element, and indirect-call signature mismatch. Unmapped mini-runtime errors or Wasmtime traps fail closed rather than collapsing into generic trap equivalence.

A deterministic generated numeric tranche emits 96 i32 modules from a committed seed across wrapping add/sub/mul and bitwise and/or/xor. Every case includes the seed and case index in diagnostics, and each compiled module is executed unchanged by both engines.

A deterministic stateful tranche emits 64 modules combining a mutable i32 global with persistent linear memory. Each module is invoked four times on one mini-runtime instance and one Wasmtime instance. The reference recurrence independently predicts the full four-call sequence, which checks state persistence, wrapping updates, memory load/store behavior, and cross-invocation observability together.

The differential workflow now runs every integration target in the nested workspace, so future differential tranches do not require editing the CI command to become active.

Wasmtime and WAT remain confined to the nested workspace and dedicated Ubuntu CI job; no product dependency, workspace member, or Rust 1.81 MSRV change is introduced. Generated table transitions, larger multi-value/stateful corpora, broader trap taxonomy, and minimized differential regressions remain follow-up work.
