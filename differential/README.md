# Reference differential execution

This nested workspace compares the mini runtime against Wasmtime without adding a reference engine to the product workspace.

The fixed corpus exercises supported semantics through exported `run` functions and compares normalized observable outcomes. It covers integer wrapping/rotation, `nop`/`drop`/`select`, indexed/default `br_table`, float bit semantics, typed memory store/load, `memory.grow`, and representative runtime traps.

A deterministic generated tranche emits 96 i32 modules from a committed seed across wrapping add/sub/mul and bitwise and/or/xor. Each generated WAT program is compiled once and the exact same bytes are executed in both engines.

A table/indirect-call tranche normalizes three additional shared trap classes: table out-of-bounds, indirect call to an uninitialized element, and indirect-call signature mismatch. A deterministic stateful tranche generates 64 modules combining mutable globals with persistent linear-memory updates and invokes every instance four times in both engines, checking the complete result sequence rather than only a single call.

The next generated tranche adds 64 stateful table-dispatch modules whose mutable selector alternates between two initialized funcref targets over six calls, plus 96 structured multi-value `if` modules returning exact `(i32, i64)` pairs. These cases compare full cross-invocation table dispatch sequences and multi-value result ordering against independent expectations in both engines.

An imported/shared-state tranche adds 48 deterministic modules backed by host-owned mutable globals and linear memories. It checks repeated guest updates, mid-sequence host overrides, exact multi-value results, and host-visible backing state after every call. A two-instance case additionally binds the same global and memory into two live instances per engine and alternates execution between them to verify cross-instance aliasing.

An imported-function tranche adds 48 deterministic stateful host-callback modules with exact cross-call host-state recurrence, a two-instance shared callback-state case, and a mixed `i32`/`i64`/`f32`/`f64` ABI case. The mini runtime and Wasmtime must agree on every guest-visible result and host-visible state transition.

An imported-table tranche compares deterministic indirect dispatch through a host-owned funcref table, active element initialization, same-instance host relocation/nulling of table entries, null-call traps, and import-limit matching against Wasmtime. Cross-instance sharing of one imported table remains intentionally excluded because mini-runtime function references are instance-bound and one `TableHandle` cannot currently back two live instances.

A host-memory tranche exercises imported callbacks that read and write guest linear memory. Across 96 deterministic updates, the callback return value and the guest's immediate `i32.load` must match an independent state model in both engines. Mini-only guards additionally verify that `NONE` denies reads, `MEMORY_READ` denies writes, and out-of-bounds host access fails without mutating memory.

A host-failure tranche interleaves 96 successful and rejected imported callback invocations. Both callbacks increment host-owned state before returning; rejected calls must normalize to one `callback-rejected` semantic class, leave guest state unchanged, and allow later calls to execute normally. The mini runtime is matched by its typed `RuntimeError::HostCallFailed`/`HostError::Message` boundary, while Wasmtime is classified by downcasting the original custom host error type rather than inspecting diagnostic text.

An automatic capture tranche generates 64 additional i32 arithmetic/bitwise/shift/rotate cases and carries a deterministic reducer that only accepts strictly simpler operand values while the same mini-vs-Wasmtime mismatch remains reproducible. A mismatch is eligible for capture only when Wasmtime also agrees with the independent Rust-side model, preventing an untrusted reference/model disagreement from being promoted as a mini-runtime regression. The minimized WAT, a `manifest.tsv`-compatible row, and provenance metadata are written under `differential/target/differential-captures/`; failed differential CI uploads that directory as a seven-day artifact when files exist.

The memory capture tranche extends that pipeline beyond pure numeric expressions. Ninety-six deterministic one-page store/load modules span successful effective addresses and exact memory-out-of-bounds traps across multiple memarg offsets. The independent model computes the unsigned effective address and four-byte width before either engine runs. Eligible mismatches are shrunk across address, offset, and stored value while preserving reference/model agreement; captures emit either an `i32` manifest expectation or the existing `memory_out_of_bounds` trap class.

The multi-value capture tranche adds 96 structured `if (result i32 i64)` cases with both then and else branches forced into the corpus. The independent model selects the exact `(i32, i64)` pair from the condition before either engine runs. Reference-backed disagreements are shrunk across the condition and all four branch constants under a strictly decreasing rank, including constants from the inactive branch, and emit the existing `pair_i32_i64` replay-manifest form.

The table capture tranche adds 96 deterministic `call_indirect` modules over a two-slot funcref table. The corpus forces successful slot-0/slot-1 dispatch, an uninitialized slot-1 null trap, and table out-of-bounds selectors. The independent model predicts either the exact target result, `indirect_call_to_null`, or `table_out_of_bounds`; reference-backed mismatches shrink selector, optional second-slot initialization, and both target constants before emitting the existing replay vocabulary.

The imported-function capture tranche crosses the host boundary with 48 deterministic stateful `i64 -> i64` callbacks and one-to-five-call traces. The independent model predicts every guest-visible XOR result and the final host-owned wrapping state. Reference-backed disagreements shrink the call-sequence length, initial host state, guest salt, and individual inputs under a strictly decreasing rank. Because the existing four-field replay manifest intentionally assumes no imports, captures instead emit a companion `.import.tsv` driver describing the host behavior, initial state, inputs, expected result trace, and final state alongside the minimized WAT and provenance metadata.

An import-aware regression replay corpus now lives under `tests/fixtures/import_regressions/`. Its eight-field manifest is directly compatible with the driver row emitted by imported-function captures and makes host behavior, initial state, salt, inputs, expected result trace, and final host state reviewable in one place. The harness independently recomputes the full `stateful_i64_add` recurrence before execution, compiles each WAT once, and requires the mini runtime and Wasmtime to reproduce the same complete trace. Two seeded ordinary/wrapping fixtures establish the promotion path without claiming previously observed production bugs.

A manifest-driven regression replay corpus keeps small no-import WAT reproducers under `tests/fixtures/regressions/`. The initial 10 seeded fixtures cover control-flow result preservation, signed-zero float semantics, multi-value ordering, memory and table bounds traps, integer arithmetic traps, invalid conversion, and indirect-call null/signature failures. The manifest records exact normalized expectations, and the runner requires the mini runtime and Wasmtime to agree with them. Seeded fixtures are regression guards, not claims of previously observed bugs.

## Boundary

- Test modules must parse, validate, and instantiate in the mini runtime before execution; a validation failure is not counted as a runtime trap.
- Successful scalar and supported multi-value results are compared exactly.
- Supported trap cases are normalized to semantic classes rather than diagnostic strings. Current shared classes cover memory/table out-of-bounds, integer overflow, integer division by zero, invalid float-to-integer conversion, null indirect calls, indirect-call signature mismatch, and explicit imported callback rejection.
- Any unmapped runtime error or Wasmtime trap fails closed instead of being treated as generic trap equivalence.
- Stateful cases reuse one instance per engine across repeated calls so mutable globals, memory persistence, table dispatch state, imported host-owned state, and imported callback state participate in observable results.
- Imported global/memory cases compare both guest-visible outputs and host-visible backing values; the shared-instance fixture verifies that two live instances observe the same imported backing.
- Imported-function cases compare deterministic host callback side effects as well as typed ABI values; the shared-state fixture alternates calls between two live guest instances bound to one host state per engine.
- Imported-table cases compare guest-visible indirect-call results, host mutation visibility, null traps, and limit compatibility. They do not pretend that cross-instance imported-table aliasing exists where the mini runtime explicitly rejects it.
- Host-memory cases compare permitted read/write behavior against Wasmtime while treating the mini runtime's explicit capability policy as its own fail-closed security boundary.
- Host-failure cases compare explicit callback rejection by typed semantic identity on the reference side, verify host side effects that occur before the rejection, and prove that guest instructions after the failed call do not execute.
- Automatic capture is fail-closed: it shrinks only a real cross-engine mismatch whose Wasmtime result matches the independent model, and it never edits the committed regression corpus by itself.
- Memory capture additionally requires exact typed memory-OOB normalization and shrinks only address/offset/value candidates whose lexicographic complexity is strictly lower.
- Multi-value capture requires Wasmtime to match the independently selected pair and shrinks condition/branch constants only through strictly lower-ranked candidates.
- Table capture requires exact result/null/OOB oracle agreement and only accepts selector/initializer/value reductions with strictly lower rank.
- Imported-function capture requires the complete Wasmtime result trace and final host state to match the independent recurrence. Its `.import.tsv` row is staging metadata until reviewed and promoted into the import-regression manifest.
- Import-regression replay rejects malformed eight-field rows, duplicate IDs/fixtures, unsafe/non-WAT paths, missing files, unsupported host behavior, empty traces, malformed i64 values, result/input length mismatches, and expectations that disagree with the independent recurrence.
- No-import regression replay rejects malformed manifest rows, duplicate IDs or fixture paths, unsafe/non-WAT paths, missing files, unknown kinds/classes, unexpected result shapes, and unmapped traps.
- Wasmtime and WAT tooling live only in this nested test workspace. They are not product dependencies and do not change the Rust 1.81 product MSRV.
- Differential CI runs every integration target under `differential/tests/`.

Run locally with:

```bash
cargo test --manifest-path differential/Cargo.toml -- --nocapture
```

Captured no-import and imported-function artifacts now both have reviewed replay destinations, but promotion remains deliberate and CI never edits committed fixtures. Future expansion should broaden import capture/replay to host globals, memory, and tables, extend stable typed host-failure normalization, and automate more review assistance without silently committing CI output.
