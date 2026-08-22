# Phase 6 reference differential tranche

The repository now has an isolated reference-engine test workspace under `differential/`. It compiles small supported WebAssembly modules from WAT once, executes the exact same bytes in the mini runtime and Wasmtime, normalizes observable results, and compares both engines against explicit expected outcomes.

The first corpus covers integer wrapping and rotation, `nop`/`drop`/untyped `select`, indexed and default `br_table`, f32/f64 bit-level edge behavior, typed memory round trips, `memory.grow`, integer divide-by-zero, memory out-of-bounds, and invalid float-to-integer conversion.

Successful i32/i64 results are bit-exact. Runtime trap cases require both engines to trap after the mini parser/validator/instantiator has already accepted the module, preventing validation rejection from being miscounted as equivalent execution behavior.

Wasmtime and WAT are deliberately confined to the nested workspace and dedicated Ubuntu CI job; no product dependency, workspace member, or Rust 1.81 MSRV change is introduced. Exact cross-engine trap taxonomy, generated differential modules, and larger stateful/multi-value corpora remain follow-up work.
