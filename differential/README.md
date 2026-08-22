# Reference differential execution

This nested workspace compares the mini runtime against Wasmtime without adding a reference engine to the product workspace.

The initial corpus exercises supported semantics through exported `run` functions and compares normalized observable outcomes. It covers integer wrapping/rotation, `nop`/`drop`/`select`, indexed/default `br_table`, float bit semantics, typed memory store/load, `memory.grow`, divide-by-zero traps, memory out-of-bounds traps, and invalid float-to-integer conversion traps.

## Boundary

- Test modules must parse, validate, and instantiate in the mini runtime before execution; a validation failure is not counted as a runtime trap.
- Successful i32/i64 results are compared exactly.
- Trap cases currently compare the observable trap/non-trap boundary, not engine-specific diagnostic strings or internal trap enum names.
- Memory behavior is compared through observable Wasm loads and `memory.grow` results rather than engine-private memory representations.
- Wasmtime and WAT tooling live only in this nested test workspace. They are not product dependencies and do not change the Rust 1.81 product MSRV.

Run locally with:

```bash
cargo test --manifest-path differential/Cargo.toml --test reference -- --nocapture
```

Future expansion should add generated deterministic modules, exact normalized trap classes, multi-value/reference-table cases where supported, and minimized regressions for every discovered mismatch.
