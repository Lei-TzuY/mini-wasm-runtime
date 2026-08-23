# Reference differential execution

This nested workspace compares the mini runtime against Wasmtime without adding a reference engine to the product workspace.

The fixed corpus exercises supported semantics through exported `run` functions and compares normalized observable outcomes. It covers integer wrapping/rotation, `nop`/`drop`/`select`, indexed/default `br_table`, float bit semantics, typed memory store/load, `memory.grow`, and representative runtime traps.

A deterministic generated tranche emits 96 i32 modules from a committed seed across wrapping add/sub/mul and bitwise and/or/xor. Each generated WAT program is compiled once and the exact same bytes are executed in both engines.

A table/indirect-call tranche normalizes three additional shared trap classes: table out-of-bounds, indirect call to an uninitialized element, and indirect-call signature mismatch. A deterministic stateful tranche generates 64 modules combining mutable globals with persistent linear-memory updates and invokes every instance four times in both engines, checking the complete result sequence rather than only a single call.

The next generated tranche adds 64 stateful table-dispatch modules whose mutable selector alternates between two initialized funcref targets over six calls, plus 96 structured multi-value `if` modules returning exact `(i32, i64)` pairs. These cases compare full cross-invocation table dispatch sequences and multi-value result ordering against independent expectations in both engines.

An imported/shared-state tranche adds 48 deterministic modules backed by host-owned mutable globals and linear memories. It checks repeated guest updates, mid-sequence host overrides, exact multi-value results, and host-visible backing state after every call. A two-instance case additionally binds the same global and memory into two live instances per engine and alternates execution between them to verify cross-instance aliasing.

A manifest-driven regression replay corpus keeps small WAT reproducers under `tests/fixtures/regressions/`. The initial 10 seeded fixtures cover control-flow result preservation, signed-zero float semantics, multi-value ordering, memory and table bounds traps, integer arithmetic traps, invalid conversion, and indirect-call null/signature failures. The manifest records exact normalized expectations, and the runner requires the mini runtime and Wasmtime to agree with them. Seeded fixtures are regression guards, not claims of previously observed bugs.

## Boundary

- Test modules must parse, validate, and instantiate in the mini runtime before execution; a validation failure is not counted as a runtime trap.
- Successful scalar and supported multi-value results are compared exactly.
- Supported trap cases are normalized to semantic classes rather than diagnostic strings. Current shared classes cover memory/table out-of-bounds, integer overflow, integer division by zero, invalid float-to-integer conversion, null indirect calls, and indirect-call signature mismatch.
- Any unmapped runtime error or Wasmtime trap fails closed instead of being treated as generic trap equivalence.
- Stateful cases reuse one instance per engine across repeated calls so mutable globals, memory persistence, table dispatch state, and imported host-owned state participate in observable results.
- Imported global/memory cases compare both guest-visible outputs and host-visible backing values; the shared-instance fixture verifies that two live instances observe the same imported backing.
- Regression replay rejects malformed manifest rows, duplicate IDs/paths, unsafe fixture paths, missing files, unknown outcome kinds/classes, unexpected result shapes, and unmapped traps.
- Wasmtime and WAT tooling live only in this nested test workspace. They are not product dependencies and do not change the Rust 1.81 product MSRV.
- Differential CI runs every integration target under `differential/tests/`.

Run locally with:

```bash
cargo test --manifest-path differential/Cargo.toml -- --nocapture
```

Future expansion should automatically capture and shrink real differential mismatches into this replay format, broaden imported-function/table combinations, add more trap normalization, and extend stateful multi-value sequences.
