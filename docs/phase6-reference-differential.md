# Phase 6 reference differential tranche

The repository has an isolated reference-engine test workspace under `differential/`. It compiles supported WebAssembly modules from WAT, executes the exact same bytes in the mini runtime and Wasmtime, normalizes observable outcomes, and compares both engines against explicit expectations.

The fixed corpus covers integer wrapping and rotation, `nop`/`drop`/untyped `select`, indexed/default `br_table`, f32/f64 bit-level edge behavior, typed memory round trips, `memory.grow`, and representative execution traps.

The differential harness compares exact normalized trap classes shared by both engines. The original classes cover memory out-of-bounds, signed integer division overflow, integer division by zero, and invalid float-to-integer conversion. The table tranche adds table out-of-bounds, indirect call to an uninitialized element, and indirect-call signature mismatch. Unmapped mini-runtime errors or Wasmtime traps fail closed rather than collapsing into generic trap equivalence.

A deterministic generated numeric tranche emits 96 i32 modules from a committed seed across wrapping add/sub/mul and bitwise and/or/xor. Every case includes the seed and case index in diagnostics, and each compiled module is executed unchanged by both engines.

A deterministic stateful tranche emits 64 modules combining a mutable i32 global with persistent linear memory. Each module is invoked four times on one mini-runtime instance and one Wasmtime instance. The reference recurrence independently predicts the full four-call sequence, which checks state persistence, wrapping updates, memory load/store behavior, and cross-invocation observability together.

A table-dispatch state tranche emits 64 modules with two initialized funcref targets and a mutable selector that alternates the `call_indirect` target across six invocations. Both engines must produce the exact alternating target-result sequence. A separate structured multi-value tranche emits 96 `if (result i32 i64)` modules and checks exact value ordering and branch selection through the mini runtime's values API and Wasmtime's typed tuple ABI.

The imported/shared-state tranche emits 48 deterministic modules importing a mutable i32 global and a bounded linear memory. Host code seeds both objects before instantiation, applies a second external override in the middle of each five-call sequence, and checks both guest-returned `(i32, i32)` results and host-visible backing state after every invocation. A dedicated two-instance fixture links the same imported global and memory into two live instances in each engine and alternates calls between them, verifying that guest writes in one instance are observed by the other and by the host.

The imported-function tranche emits 48 deterministic modules importing an `i64 -> i64` host callback. Each callback owns host-side wrapping state, every module is invoked five times, and an independent recurrence predicts both the callback state and the guest-visible post-call XOR result. A second fixture binds two guest instances to one host state per engine and alternates calls between them, while a mixed numeric ABI fixture checks `i32`, `i64`, `f32`, and `f64` parameter ordering and bit-preserving host observation against Wasmtime.

The initial differential regression replay layer adds a manifest and 10 manually minimized seeded WAT reproducers. It covers `br_table` default-result preservation, f64 signed-zero `min`, structured `(i32, i64)` ordering, memory OOB, signed integer divide-by-zero and overflow, invalid float-to-integer conversion, null indirect calls, table OOB, and indirect-call signature mismatch. The runner validates manifest integrity and path safety, compiles each WAT once, then requires the exact same Wasm bytes to match the declared normalized result in both engines. These seeded reproducers establish the replay format without implying that each fixture came from a previously observed bug.

The differential workflow runs every integration target in the nested workspace, so future differential tranches do not require editing the CI command to become active.

Wasmtime and WAT remain confined to the nested workspace and dedicated Ubuntu CI job; no product dependency, workspace member, or Rust 1.81 MSRV change is introduced. Automatic capture/shrinking of real mismatches, broader imported-table scenarios, richer host-failure/capability differentials, broader multi-value/stateful corpora, and additional trap taxonomy remain follow-up work.
