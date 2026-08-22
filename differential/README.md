# Reference differential execution

This nested workspace compares the mini runtime against Wasmtime without adding a reference engine to the product workspace.

The fixed corpus exercises supported semantics through exported `run` functions and compares normalized observable outcomes. It covers integer wrapping/rotation, `nop`/`drop`/`select`, indexed/default `br_table`, float bit semantics, typed memory store/load, `memory.grow`, and representative runtime traps.

A deterministic generated tranche additionally emits 96 i32 modules from a committed seed across wrapping add/sub/mul and bitwise and/or/xor. Each generated WAT program is compiled once and the exact same bytes are executed in both engines.

## Boundary

- Test modules must parse, validate, and instantiate in the mini runtime before execution; a validation failure is not counted as a runtime trap.
- Successful i32/i64 results are compared exactly.
- Supported trap cases are normalized to semantic classes rather than diagnostic strings. Current shared classes are memory out-of-bounds, integer overflow, integer division by zero, and invalid float-to-integer conversion.
- Any unmapped runtime error or Wasmtime trap fails closed instead of being treated as generic trap equivalence.
- Memory behavior is compared through observable Wasm loads and `memory.grow` results rather than engine-private memory representations.
- Wasmtime and WAT tooling live only in this nested test workspace. They are not product dependencies and do not change the Rust 1.81 product MSRV.

Run locally with:

```bash
cargo test --manifest-path differential/Cargo.toml --test reference -- --nocapture
```

Future expansion should add table/indirect-call trap classes, generated structured and stateful modules, multi-value cases, and minimized regression fixtures for every discovered mismatch.
