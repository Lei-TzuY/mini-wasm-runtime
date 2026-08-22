# Phase 6 reference differential tranche

The repository has an isolated reference-engine test workspace under `differential/`. It compiles supported WebAssembly modules from WAT, executes the exact same bytes in the mini runtime and Wasmtime, normalizes observable outcomes, and compares both engines against explicit expectations.

The fixed corpus covers integer wrapping and rotation, `nop`/`drop`/untyped `select`, indexed/default `br_table`, f32/f64 bit-level edge behavior, typed memory round trips, `memory.grow`, and representative execution traps.

The differential harness now compares four exact normalized trap classes shared by both engines: memory out-of-bounds, signed integer division overflow, integer division by zero, and invalid float-to-integer conversion. Unmapped mini-runtime errors or Wasmtime traps fail closed rather than collapsing into generic trap equivalence.

A deterministic generated tranche emits 96 i32 modules from a committed seed across wrapping add/sub/mul and bitwise and/or/xor. Every case includes the seed and case index in diagnostics, and each compiled module is executed unchanged by both engines.

Wasmtime and WAT remain confined to the nested workspace and dedicated Ubuntu CI job; no product dependency, workspace member, or Rust 1.81 MSRV change is introduced. Table/indirect-call trap normalization, generated structured/stateful modules, larger multi-value corpora, and minimized differential regressions remain follow-up work.
