# Reference differential execution

This nested workspace compares the mini runtime against Wasmtime without adding a reference engine to the product workspace.

The fixed corpus exercises supported semantics through exported `run` functions and compares normalized observable outcomes. It covers integer wrapping/rotation, `nop`/`drop`/`select`, indexed/default `br_table`, float bit semantics, typed memory store/load, `memory.grow`, and representative runtime traps.

A deterministic generated tranche emits 96 i32 modules from a committed seed across wrapping add/sub/mul and bitwise and/or/xor. Each generated WAT program is compiled once and the exact same bytes are executed in both engines.

A table/indirect-call tranche normalizes three additional shared trap classes: table out-of-bounds, indirect call to an uninitialized element, and indirect-call signature mismatch. A separate deterministic stateful tranche generates 64 modules combining mutable globals with persistent linear-memory updates and invokes every instance four times in both engines, checking the complete result sequence rather than only a single call.

## Boundary

- Test modules must parse, validate, and instantiate in the mini runtime before execution; a validation failure is not counted as a runtime trap.
- Successful i32/i64 results are compared exactly.
- Supported trap cases are normalized to semantic classes rather than diagnostic strings. Current shared classes cover memory/table out-of-bounds, integer overflow, integer division by zero, invalid float-to-integer conversion, null indirect calls, and indirect-call signature mismatch.
- Any unmapped runtime error or Wasmtime trap fails closed instead of being treated as generic trap equivalence.
- Stateful cases reuse one instance per engine across repeated calls so mutable globals and memory persistence participate in the observable result.
- Wasmtime and WAT tooling live only in this nested test workspace. They are not product dependencies and do not change the Rust 1.81 product MSRV.
- Differential CI runs every integration target under `differential/tests/`.

Run locally with:

```bash
cargo test --manifest-path differential/Cargo.toml -- --nocapture
```

Future expansion should add generated table/state transitions, multi-value cases, broader trap normalization, and minimized regression fixtures for every discovered mismatch.
