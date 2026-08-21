# Phase 5C — Broader Module Forms + Conformance

Phase 5C broadens the module surface only where the runtime can preserve WebAssembly's index spaces and instantiation semantics exactly. Unsupported proposal features continue to fail closed.

## Goals

1. Extend imports beyond functions without collapsing distinct WebAssembly index spaces.
2. Support block type indices for the subset that fits the runtime's current zero-or-one-result execution model.
3. Broaden segment parsing only when the corresponding runtime semantics exist.
4. Add reusable binary-fixture helpers and adversarial cross-layer tests for supported behavior.
5. Keep host capabilities explicit: importing memory/table/global objects must never grant filesystem, network, process, or environment access.

## Index spaces

WebAssembly maintains independent function, table, memory, and global index spaces. Imported objects precede defined objects within their own index space. Phase 5C must model this explicitly rather than applying function-import arithmetic to other kinds.

Required invariants:
- function calls/exports/elements resolve in the function index space;
- table instructions/exports/elements resolve in the table index space;
- memory instructions/exports/data segments resolve in the memory index space;
- global instructions/exports resolve in the global index space;
- imported and defined objects are type-checked before instantiation;
- code bodies still correspond only to defined functions.

## Import surface

The parser may decode function, table, memory, and global imports. Instantiation must resolve every imported object explicitly from an embedding-provided registry or store.

The initial Phase-5C implementation should prefer immutable imported globals and exact imported table/memory types before adding mutable aliasing. Mutable imported state has observable aliasing semantics and must not be approximated by copying values.

If the host API cannot express the required aliasing safely and clearly, that import form remains rejected rather than silently copied.

## Block type indices

MVP block types encode either `0x40` or a value type. The multi-value extension also allows a signed type index. Phase 5C may accept a type-index block signature only when:
- the referenced function type exists;
- its parameter list can be represented by the implemented control-frame model;
- its result list has at most one supported numeric value;
- branch label typing follows the exact block/loop/if signature.

A type-index signature that requires unsupported multi-value behavior must be rejected explicitly.

## Segment forms

Active data/element segments are already supported in narrow forms. Passive or declarative segments are useful only when matching bulk-memory/reference-type instructions exist. Merely parsing them and then ignoring them would be incorrect.

Therefore Phase 5C may decode additional segment modes into the AST for inspection, but executable modules containing passive segments must remain rejected until their instructions/instantiation semantics are implemented. No segment is silently dropped.

## Conformance strategy

Conformance work is scoped to the supported feature set:
- minimal hand-built binary fixtures for each accepted form;
- negative fixtures for malformed encodings, bad indices, type mismatches, and unsupported proposal combinations;
- cross-layer tests that parse -> validate -> instantiate -> execute when execution exists;
- runtime defense-in-depth tests for malformed host bindings and dynamic bounds/type errors.

Reference-engine differential testing remains Phase 6; Phase 5C must not add Wasmtime/Wasmer as a runtime dependency.

## Non-goals

- WASI
- multi-value execution
- shared memory / threads
- memory64
- GC/reference-types beyond the existing funcref table subset
- bulk-memory instructions unless implemented as a complete vertical slice
- implicit host capabilities
- JIT compilation
