# Phase 5C — Broader Module Forms + Conformance

Phase 5C broadens the module surface only where the runtime can preserve WebAssembly's index spaces and instantiation semantics exactly. Unsupported proposal features continue to fail closed.

## Current completed slices

The current Phase-5C branch has completed four vertical slices:

1. **Type-index block signatures.** Signed-33 blocktype decoding, block parameters, zero-or-one numeric results, loop parameter label types, if/else parameter restoration, and runtime control metadata all use the referenced function type exactly.
2. **Independent import index spaces.** The parser retains function/table/memory/global import descriptors in binary order while the validator resolves each kind in its own WebAssembly index space. Object imports do not shift function indices.
3. **Immutable numeric global imports.** Embedders can register immutable i32/i64/f32/f64 globals explicitly. Instantiation checks binding presence, exact value type, and immutability.
4. **Shared mutable global imports.** `GlobalHandle` provides single-threaded shared backing. Host writes are visible to WebAssembly and WebAssembly `global.set` writes are visible through the host's retained handle. Type and mutability must match the imported `GlobalType` exactly.

Table and memory imports are parsed and validated but deliberately rejected at instantiation until equivalent shared backing preserves identity, growth, and mutation visibility.

## Goals

1. Extend imports beyond functions without collapsing distinct WebAssembly index spaces.
2. Support block type indices for the subset that fits the runtime's current zero-or-one-result execution model.
3. Broaden segment parsing only when the corresponding runtime semantics exist.
4. Add reusable binary-fixture helpers and adversarial cross-layer tests for supported behavior.
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
- runtime global storage follows imported-globals-then-defined-globals ordering;
- non-function imports never alter function ordinals.

## Import surface

The parser decodes function, table, memory, and global imports.

Execution support is intentionally narrower:
- function imports resolve through the existing host-function registry and remain i32-only;
- immutable numeric globals may be registered directly with `HostRegistry::register_immutable_global`;
- mutable or immutable globals may be registered through a `GlobalHandle` with `HostRegistry::register_global`;
- table and memory imports remain runtime errors.

Global binding performs exact `ValueType` and mutability matching. Missing bindings, type mismatches, and mutability mismatches are distinct errors.

### Shared global identity

`GlobalHandle` owns an `Rc<RefCell<Value>>` plus its declared runtime value type and mutability. The runtime itself is currently single-threaded, so this representation deliberately models single-threaded aliasing rather than claiming thread-safe shared-memory semantics.

For an imported mutable global:

```text
host GlobalHandle clone ----+
                            |
                            +---- same shared cell ---- Instance global index
```

The host may update the handle between calls and the next WebAssembly `global.get` sees the new value. Conversely, WebAssembly `global.set` updates the shared cell observed by the host. The handle rejects writes to immutable globals and rejects values of the wrong numeric type.

Defined globals use the same internal handle abstraction but remain instance-owned because no public alias is created for them.

The next object-import step requires equivalent shared backing for tables or memories. Copying their contents would be incorrect because table/memory identity, mutation visibility, and memory growth are observable.

## Block type indices

MVP block types encode either `0x40` or a value type. The multi-value extension also allows a signed type index. Phase 5C accepts a type-index block signature when:
- the referenced function type exists;
- its parameters are supported numeric types;
- its result list has at most one supported numeric value;
- branch label typing follows the exact block/loop/if signature.

For blocks and ifs, branch labels carry the result types. Loop labels carry the block parameter types. Each if arm starts with the declared block parameters. A type-index signature requiring multiple result values remains explicitly rejected.

## Segment forms

Active data/element segments are already supported in narrow forms. Passive or declarative segments are useful only when matching bulk-memory/reference-type instructions exist. Merely parsing them and then ignoring them would be incorrect.

Therefore additional segment modes remain deferred unless their complete parser -> validator -> instantiation/execution semantics are implemented. No segment is silently dropped.

## Conformance strategy

Conformance work is scoped to the supported feature set:
- minimal hand-built binary fixtures for each accepted form;
- negative fixtures for malformed encodings, bad indices, type mismatches, and unsupported proposal combinations;
- cross-layer tests that parse -> validate -> instantiate -> execute when execution exists;
- runtime defense-in-depth tests for malformed host bindings and dynamic bounds/type errors;
- mixed-import fixtures specifically checking that one object kind cannot perturb another kind's index space;
- aliasing fixtures that verify mutable imported state is observable from both sides of the host/runtime boundary.

Current Phase-5C integration coverage includes signed-33 boundaries, multi-byte type indices, block parameters, loop label parameters, if/else restoration, missing/multi-result block types, mixed import ordering, imported object index visibility, fail-closed table/memory imports, immutable global resolution, exact global type/mutability matching, imported/defined global ordering, `GlobalHandle` write validation, and bidirectional mutable-global aliasing.

Reference-engine differential testing remains Phase 6; Phase 5C must not add Wasmtime/Wasmer as a runtime dependency.

## Non-goals

- WASI
- multi-value execution
- shared memory / threads
- memory64
- GC/reference-types beyond the existing funcref table subset
- bulk-memory instructions unless implemented as a complete vertical slice
- implicit host capabilities
- copy-based emulation of table or memory imports
- JIT compilation
