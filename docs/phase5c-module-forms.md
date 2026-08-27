# Phase 5C — Broader Module Forms + Conformance

Phase 5C broadens the module surface only where the runtime can preserve WebAssembly's index spaces, typing, object identity, and instantiation semantics exactly. Unsupported proposal features continue to fail closed.

## Current completed slices

The current Phase-5C branch has completed eleven major vertical slices and continues to deepen conformance within those boundaries:

1. **Type-index block signatures.** Signed-33 blocktype decoding, block parameters, zero-or-one numeric results, loop parameter label types, if/else parameter restoration, and runtime control metadata all use the referenced function type exactly.
2. **Independent import index spaces.** The parser retains function/table/memory/global import descriptors in binary order while the validator resolves each kind in its own WebAssembly index space. Object imports do not shift function indices.
3. **Numeric global imports.** Immutable and mutable i32/i64/f32/f64 imports use explicit host bindings and shared `GlobalHandle` backing with exact type and mutability matching.
4. **Shared imported tables.** `TableHandle` gives imported `funcref` tables host-visible shared backing. Active element segments update the same table after all-segment preflight, host slot changes are immediately visible to `call_indirect`, and opaque instance-bound `FunctionRef` values fail closed when stale or foreign.
5. **Shared imported memory.** `MemoryHandle` gives imported memories shared host/Wasm backing. Host and Wasm observe the same bytes, current page count, growth, and maximum; import-limit matching and runtime caps are enforced before instantiation.
6. **Imported-object adversarial hardening.** Active data/element initialization preflights every segment before mutating shared backing. Failed table preflight does not poison a retained handle, and capability-gated host callbacks access the exact same imported memory retained by the embedding host.
7. **Negative conformance hardening.** Cross-layer malformed and invalid fixtures lock in rejection of duplicate/out-of-order sections, function/code cardinality mismatch, bad index spaces and instruction immediates, control-stack/type errors, memory misuse, global index/mutability/initializer violations, unsupported segment modes, and segment target/offset errors.
8. **Curated supported-spec vectors.** Source-faithful vectors derived from `WebAssembly/spec` at pinned commit `fc209c5ed8afc4dfeb9252024d217da3376c7a6f` exercise supported numeric, function/control, `call_indirect`, memory/grow/page-end/memarg-offset, active segment, and numeric-global semantics without claiming unsupported proposal features or silently filtering invalid cases.
9. **Untyped numeric `select` (`0x1b`).** Validator stack typing preserves unreachable-stack polymorphism and runtime execution supports i32/i64/f32/f64 values. Zero/nonzero choice, global/state/control/call/memory/numeric contexts, candidate/condition type errors, reachable underflow, NaN payload preservation, and the typed-select boundary are explicitly tested.
10. **Bit-exact reinterpret (`0xbc..0xbf`).** Validator/runtime/control-map admission is complete for `i32.reinterpret_f32`, `i64.reinterpret_f64`, `f32.reinterpret_i32`, and `f64.reinterpret_i64`. Runtime uses exact `to_bits`/`from_bits` semantics, preserving NaN payloads and signed zero, with wrong-type, reachable-underflow, structured-control, and unreachable-polymorphism coverage.
11. **MVP `drop` (`0x1a`).** Validation consumes one value with the existing current-frame polymorphic `pop_any` rules, runtime discards exactly one top value, and control-map scanning recognizes the opcode. i32/i64/f32/f64 payloads, lower-result preservation, reachable underflow, and unreachable-stack polymorphism are covered.

## Goals

1. Extend imports beyond functions without collapsing distinct WebAssembly index spaces.
2. Support block type indices for the subset that fits the runtime's current zero-or-one-result execution model.
3. Broaden segment parsing only when the corresponding runtime semantics exist.
4. Add reusable binary fixtures and adversarial cross-layer tests for supported behavior.
5. Keep host capabilities explicit: importing memory/table/global objects must never grant filesystem, network, process, or environment access.

## Index spaces

WebAssembly maintains independent function, table, memory, and global index spaces. Imported objects precede defined objects within their own index space. Phase 5C models this explicitly rather than applying function-import arithmetic to other kinds.

Current invariants:
- function calls/exports/elements resolve in the function index space;
- table instructions/exports/elements resolve in the table index space;
- memory instructions/exports/data segments resolve in the memory index space;
- global instructions/exports resolve in the global index space;
- imported and defined objects are type-checked before instantiation;
- code bodies correspond only to defined functions;
- runtime storage follows imported-then-defined ordering within each object index space;
- non-function imports never alter function ordinals.

## Import surface

The parser decodes function, table, memory, and global imports.

Execution support is intentionally narrower than the full WebAssembly specification:
- function imports resolve through the explicit host-function registry and remain i32-only with zero-or-one result;
- immutable numeric globals may be registered directly with `HostRegistry::register_immutable_global`;
- mutable or immutable globals may be registered through a shared `GlobalHandle` with `HostRegistry::register_global`;
- `funcref` table imports resolve through `HostRegistry::register_table` and a shared `TableHandle`;
- memory imports resolve through `HostRegistry::register_memory` and a shared `MemoryHandle`.

Missing bindings and incompatible host object types/limits are explicit instantiation errors. Imported mutable state is never approximated with copy semantics.

### Shared global identity

`GlobalHandle` owns an `Rc<RefCell<Value>>` plus its declared runtime value type and mutability. The runtime is single-threaded, so this representation deliberately models single-threaded aliasing rather than WebAssembly threads/shared-memory proposal semantics.

For an imported mutable global:

```text
host GlobalHandle clone ----+
                            |
                            +---- same shared cell ---- Instance global index
```

The host may update the handle between calls and the next WebAssembly `global.get` sees the new value. Conversely, WebAssembly `global.set` updates the shared cell observed by the host. The handle rejects writes to immutable globals and values of the wrong numeric type.

Defined globals use the same internal handle abstraction but remain instance-owned because no public alias is created for them.

### Global initializer boundary

Defined globals currently store exactly one numeric literal constant (`i32.const`, `i64.const`, `f32.const`, or `f64.const`) followed by `end`. The parser verifies that the literal type matches the declared global type and rejects any non-end instruction following the literal. It also rejects non-literal initializer opcodes such as `local.get`.

This deliberately does **not** claim support for `global.get` initializers, reference-valued globals, or extended-constant-expression operators. Those forms require a richer constant-expression representation plus the corresponding validation/instantiation rules. Until that complete vertical slice exists, they remain fail closed rather than being partially parsed or evaluated.

### Shared table identity

`TableHandle` owns shared `funcref` slots and an optional maximum:

```text
host TableHandle clone -----+
                            |
                            +---- Rc<RefCell<Vec<Option<FunctionRef>>>> ---- Instance table
```

Instantiation validates the supplied table against the import limits. The current table length must be at least the imported minimum. If the import declares a maximum, the supplied handle must also declare a maximum no larger than it.

A `FunctionRef` is intentionally opaque. Internally it carries the identity of the runtime instance that produced it and that instance's function index. `call_indirect` checks this identity before dispatch. A stale reference from a dropped instance, or a reference belonging to another instance, therefore fails closed instead of invoking a different function that happens to have the same numeric index.

The current implementation binds a `TableHandle` to at most one live instance. After that instance is dropped, the handle may be reused, but stale references remain invalid. Full multi-instance store/reference semantics are deferred.

Active element initialization is visible through the host's retained `TableHandle`. Before applying any active element write, instantiation preflights every active segment range. If a later segment is out of bounds, instantiation fails without leaving earlier segment writes in the shared table. Once instantiated, host `TableHandle::set` changes are immediately observed by `call_indirect`.

### Shared memory identity

`MemoryHandle` owns the linear-memory bytes and declared maximum behind shared single-threaded backing:

```text
host MemoryHandle clone ----+
                            |
                            +---- shared byte vector / limits ---- Instance memory
```

Import matching follows WebAssembly limit subtyping for the supported memory form:
- the handle's current size must satisfy the imported minimum;
- if the import declares a maximum, the handle must declare a maximum no larger than that imported maximum;
- the handle's reachable maximum must not exceed the instance `RuntimeLimits::max_memory_pages` cap.

Host byte writes are immediately visible to Wasm loads, and Wasm stores are immediately visible through the retained host handle. `memory.size` and `memory.grow` operate on the same shared backing. One `MemoryHandle` may back multiple live instances under the current single-threaded runtime model.

Active data initialization is transactional with respect to shared memory: every active segment range is preflighted before any segment is copied. If a later segment is out of bounds, no earlier segment is left partially applied. Host callbacks granted explicit memory capabilities read and write this same imported backing; no shadow copy exists.

For the supported i32 load/store family, effective addresses are formed from the unsigned i32 base plus the unsigned memarg offset without 32-bit wraparound. Page-end tests cover full-width and narrow 8/16-bit accesses. Out-of-bounds stores preflight the entire write before mutation, including when the backing memory is host-shared through `MemoryHandle`.

## Block type indices

MVP block types encode either `0x40` or a value type. The multi-value extension also allows a signed type index. Phase 5C accepts a type-index block signature when:
- the referenced function type exists;
- its parameters are supported numeric types;
- its result list has at most one supported numeric value;
- branch label typing follows the exact block/loop/if signature.

For blocks and ifs, branch labels carry the result types. Loop labels carry the block parameter types. Each if arm starts with the declared block parameters. A type-index signature requiring multiple result values remains explicitly rejected.

## Parametric/control boundary

Untyped numeric `select` (`0x1b`) is supported for i32/i64/f32/f64. The validator requires an i32 condition and candidate values of one common numeric type, while preserving WebAssembly's polymorphic unreachable-stack rules. Runtime execution returns the first candidate for any nonzero condition and the second candidate for zero without applying numeric conversion.

MVP `drop` (`0x1a`) is supported as a complete validator/runtime/control-map slice and removes exactly one top value without inspecting its numeric type. Typed select (`0x1c`) remains explicitly fail closed with `ValidationError::UnsupportedOpcode`.

`nop` (`0x01`) remains explicitly fail closed in reachable, structured-control, and validator-unreachable contexts. MVP `unreachable` (`0x00`) is likewise still outside the admitted instruction surface even when it appears inside a frame already marked unreachable. `br_table` (`0x0e`) also remains outside the supported opcode surface until immediate decoding, common-label typing, runtime target selection, and control-map scanning are implemented together.

## Segment forms

Active data/element segments are supported in narrow forms. Passive or declarative segments are useful only when matching bulk-memory/reference-type instructions exist. Merely parsing them and then ignoring them would be incorrect.

Therefore additional segment modes remain deferred unless their complete parser -> validator -> instantiation/execution semantics are implemented. Explicit memory/table-index mode 2 currently fails closed instead of being interpreted as legacy mode 0. No segment is silently dropped.

Active segment offsets are literal-only i32 constant expressions. Instantiation interprets their bits as unsigned i32 addresses/indices, checks against the current memory/table size rather than a declared maximum, and preflights all active writes before exposing mutations to imported backing.

## Conformance strategy

Conformance work is scoped to the supported feature set:
- minimal hand-built binary fixtures for each accepted form;
- negative fixtures for malformed encodings, bad indices, type mismatches, and unsupported proposal combinations;
- cross-layer tests that parse -> validate -> instantiate -> execute when execution exists;
- runtime defense-in-depth tests for malformed host bindings and dynamic bounds/type errors;
- mixed-import fixtures specifically checking that one object kind cannot perturb another kind's index space;
- aliasing fixtures that verify mutable imported state is observable from both sides of the host/runtime boundary;
- curated source-faithful vectors from the pinned `WebAssembly/spec` revision for semantics already implemented by the runtime;
- explicit fail-closed tests for upstream forms that fall immediately outside the supported boundary, rather than silently filtering or approximating them.

Current Phase-5C integration coverage includes signed-33 boundaries, multi-byte type indices, block parameters, loop label parameters, if/else restoration, missing/multi-result block types, mixed import ordering, imported object index visibility, immutable/global binding checks, bidirectional mutable-global aliasing, imported-table limit matching, active-element host visibility, host-to-`call_indirect` table mutation, stale-reference isolation, failed imported-table instantiation atomicity, imported-memory limit matching and runtime caps, host/Wasm memory aliasing, multi-instance shared memory, memory growth visibility, failed imported-memory instantiation atomicity, host-callback access to imported memory, exact full-width and narrow memarg effective-address boundaries, failed-store atomicity for defined and imported memory, control/index/immediate/type negative-conformance suites, numeric-global index and immutability rejection, global-initializer fail-closed behavior, untyped numeric `select` validation/execution, bit-exact reinterpret execution, MVP `drop` execution, typed-select fail-closed behavior, explicit `nop` and MVP-`unreachable` fail-closed boundaries, and curated pinned supported-spec vectors spanning numeric/function/control/indirect-call/memory/global behavior.

Reference-engine differential testing remains Phase 6; Phase 5C must not add Wasmtime/Wasmer as a runtime dependency.

## Remaining Phase 5C scope

Still intentionally deferred:
- non-i32 host function ABI, only with callback result-type checking in the same complete slice;
- passive/declarative and explicit-index data/element modes;
- multi-value execution;
- `nop`, MVP `unreachable`, `br_table`, and typed select (`0x1c`) as complete validator/runtime/control-map slices;
- broader numeric operators plus trapping float-to-integer and integer-to-float conversion execution semantics;
- i64/f32/f64 memory instruction families;
- broader and more automated upstream spec coverage beyond the current curated pinned vectors;
- richer constant expressions, including supported `global.get` initializer semantics, only as a complete parser/validator/instantiation slice;
- remaining negative-conformance coverage for unsupported reference, segment, initializer, and validation-context forms.

## Non-goals

- WASI
- shared memory / threads
- memory64
- GC/reference-types beyond the existing `funcref` table subset
- multiple live instances sharing one `TableHandle`
- cross-instance function-reference dispatch
- bulk-memory instructions unless implemented as a complete vertical slice
- implicit host capabilities
- copy-based emulation of imported mutable objects
- JIT compilation
