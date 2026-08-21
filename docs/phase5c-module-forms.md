# Phase 5C — Broader Module Forms + Conformance

Phase 5C broadens the module surface only where the runtime can preserve WebAssembly's index spaces, typing, object identity, and instantiation semantics exactly. Unsupported proposal features continue to fail closed.

## Current completed slices

The current Phase-5C branch has completed seven vertical slices:

1. **Type-index block signatures.** Signed-33 blocktype decoding, block parameters, zero-or-one numeric results, loop parameter label types, if/else parameter restoration, and runtime control metadata all use the referenced function type exactly.
2. **Independent import index spaces.** The parser retains function/table/memory/global import descriptors in binary order while the validator resolves each kind in its own WebAssembly index space. Object imports do not shift function indices.
3. **Numeric global imports.** Immutable and mutable i32/i64/f32/f64 imports use explicit host bindings and shared `GlobalHandle` backing with exact type and mutability matching.
4. **Shared imported tables.** `TableHandle` gives imported `funcref` tables host-visible shared backing. Active element segments update the same table after all-segment preflight, host slot changes are immediately visible to `call_indirect`, and opaque instance-bound `FunctionRef` values fail closed when stale or foreign.
5. **Shared imported memory.** `MemoryHandle` gives imported memories shared host/Wasm backing. Host and Wasm observe the same bytes, current page count, growth, and maximum; import-limit matching and runtime caps are enforced before instantiation.
6. **Imported-memory adversarial hardening.** Active data initialization preflights every segment before mutating shared memory, and capability-gated host callbacks access the exact same imported backing retained by the embedding host.
7. **Initial negative-conformance corpus.** Cross-layer malformed and invalid fixtures lock in rejection of duplicate/out-of-order standard sections, function/code cardinality mismatch, duplicate export names, missing start targets, and memory instructions without a linear memory.

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

## Block type indices

MVP block types encode either `0x40` or a value type. The multi-value extension also allows a signed type index. Phase 5C accepts a type-index block signature when:
- the referenced function type exists;
- its parameters are supported numeric types;
- its result list has at most one supported numeric value;
- branch label typing follows the exact block/loop/if signature.

For blocks and ifs, branch labels carry the result types. Loop labels carry the block parameter types. Each if arm starts with the declared block parameters. A type-index signature requiring multiple result values remains explicitly rejected.

## Segment forms

Active data/element segments are supported in narrow forms. Passive or declarative segments are useful only when matching bulk-memory/reference-type instructions exist. Merely parsing them and then ignoring them would be incorrect.

Therefore additional segment modes remain deferred unless their complete parser -> validator -> instantiation/execution semantics are implemented. No segment is silently dropped.

## Conformance strategy

Conformance work is scoped to the supported feature set:
- minimal hand-built binary fixtures for each accepted form;
- negative fixtures for malformed encodings, bad indices, type mismatches, and unsupported proposal combinations;
- cross-layer tests that parse -> validate -> instantiate -> execute when execution exists;
- runtime defense-in-depth tests for malformed host bindings and dynamic bounds/type errors;
- mixed-import fixtures specifically checking that one object kind cannot perturb another kind's index space;
- aliasing fixtures that verify mutable imported state is observable from both sides of the host/runtime boundary.

Current Phase-5C integration coverage includes signed-33 boundaries, multi-byte type indices, block parameters, loop label parameters, if/else restoration, missing/multi-result block types, mixed import ordering, imported object index visibility, immutable/global binding checks, bidirectional mutable-global aliasing, imported-table limit matching, active-element host visibility, host-to-`call_indirect` table mutation, stale-reference isolation, failed imported-table instantiation atomicity, imported-memory limit matching and runtime caps, host/Wasm memory aliasing, multi-instance shared memory, memory growth visibility, failed imported-memory instantiation atomicity, host-callback access to imported memory, and the initial malformed/invalid negative-conformance corpus.

Reference-engine differential testing remains Phase 6; Phase 5C must not add Wasmtime/Wasmer as a runtime dependency.

## Remaining Phase 5C scope

Still intentionally deferred:
- non-i32 host function ABI;
- broader data/element modes;
- multi-value execution;
- broader numeric operators, reinterpret, and trapping conversions;
- i64/f32/f64 memory instruction families;
- WebAssembly spec tests for supported features;
- broader negative-conformance coverage beyond the initial corpus.

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
