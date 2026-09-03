# Phase 5C — Broader Module Forms + Conformance

> Historical slice note: this document describes the repository state at this Phase-5C consolidation point. Later Phase-5C/Phase-6 work added defined-function structured multi-value execution and the `HostRegistry::register_values` multi-result host callback ABI. For current capability claims, prefer `README.md`, `docs/architecture.md`, `docs/roadmap.md`, and `docs/phase5c-host-multi-value.md`; limitations below are preserved as provenance for this slice.

Phase 5C broadens the executable WebAssembly MVP surface only where parser, validator, instantiation, and runtime semantics can remain exact. Unsupported proposal features continue to fail closed.

## Current implemented state

The stacked Phase-5C work now includes:

- signed-33 type-index block signatures with block/loop/if parameters and zero-or-one numeric result;
- independent function/table/memory/global import index spaces;
- shared imported `GlobalHandle`, `TableHandle`, and `MemoryHandle` backing rather than copy semantics;
- typed host function imports across i32/i64/f32/f64 with exact signature and result-variant checks;
- passive and explicit-index data modes plus passive/declarative/explicit-index legacy funcref element modes;
- complete MVP i32/i64 integer operator core;
- complete MVP f32/f64 operator core;
- typed i64/f32/f64 memory load/store families;
- bit-exact reinterpret instructions;
- unprefixed trapping float-to-integer and integer-to-float conversions;
- `0xfc` saturating float-to-integer conversions with u32 LEB subopcode decoding;
- a cross-layer negative-conformance corpus for parser/validator fail-closed behavior.

Multi-value execution, supported-feature spec-test ingestion, expression-based element/reference forms, bulk memory/table operations, SIMD, memory64, and multi-memory/multi-table remain outside the current completed surface.

## Index-space invariant

WebAssembly function, table, memory, and global indices are independent. Imported objects precede defined objects only inside their own index space. No object kind may satisfy or shift an index belonging to another kind.

The validator therefore resolves function calls/exports/elements in the function space, table operations/exports/elements in the table space, memory operations/exports/data segments in the memory space, and global operations/exports in the global space. The negative-conformance corpus includes adversarial table-vs-memory export fixtures to lock this separation.

## Imported mutable object identity

Imported mutable objects are shared with the embedding host rather than copied.

`GlobalHandle`, `TableHandle`, and `MemoryHandle` use single-threaded shared backing. This models observable identity and aliasing for the current interpreter; it does not claim WebAssembly threads/shared-memory proposal semantics.

For imported memory, host and Wasm observe the same bytes, current page count, growth, and maximum. Active data initialization is preflighted across all active segments before any shared memory mutation, so a later out-of-bounds segment cannot leave earlier partial writes behind.

For imported tables, active element initialization is likewise preflighted before shared table mutation. Opaque instance-bound function references prevent stale or foreign references from being treated as coincidentally equal function indices.

## Host function boundary

Host function registrations use exact numeric signatures over i32/i64/f32/f64 and remain limited to zero-or-one result. Runtime argument variants are checked before callback entry and callback result variants are checked before values can enter the WebAssembly operand stack. Floating-point payload bits are preserved at the host boundary.

Host capabilities remain explicit. Importing memory/table/global state does not implicitly grant filesystem, network, process, or environment access.

## Segment modes

Supported data modes are active implicit-memory-0, passive, and active explicit-memory-index forms. Supported legacy function-index element modes are active implicit-table-0, passive, active explicit-table-index, and declarative forms.

Passive/declarative segments are preserved but inert until the corresponding bulk-memory/table initialization instructions exist. Expression-based element forms remain fail-closed rather than being silently reinterpreted as legacy function-index vectors.

## Numeric execution

The current stack covers the MVP integer and floating-point operator cores, typed numeric memory accesses, reinterpretation, trapping float-to-integer conversions, integer-to-float conversions, and all eight saturating `trunc_sat` conversions.

Trap-sensitive operations keep WebAssembly semantics explicit: integer division-by-zero and signed overflow are checked before host-language division; trapping float conversions distinguish invalid NaN conversion from integer overflow; saturating conversions clamp instead of trapping; reinterpret instructions preserve bits exactly.

## Conformance strategy

Conformance is scoped to the supported feature set and remains fail closed:

- malformed section ordering/uniqueness and payload consumption;
- malformed or unsupported immediates and limits encodings;
- function/code cardinality and export uniqueness;
- invalid start, object, and instruction indices;
- object-index cross-space confusion;
- memory-use-without-memory and alignment/type errors;
- host binding mismatch and shared-state atomicity;
- dynamic trap boundaries for indirect calls, memory, integer arithmetic, and conversions.

The negative-conformance integration corpus is intentionally cross-layer: fixtures enter through binary parsing and are rejected by the earliest correct layer rather than relying only on direct validator-unit construction.

Reference-engine differential testing remains Phase 6; Wasmtime/Wasmer are not runtime dependencies.

## Remaining Phase 5C scope

- multi-value function/block execution;
- WebAssembly spec tests restricted to features the runtime claims to support;
- continued expansion of malformed/invalid binary fixtures as new surfaces are added.

## Later / non-goals

- WASI or implicit OS capabilities;
- threads/shared-memory proposal semantics;
- expression-based reference forms beyond the current legacy funcref subset;
- bulk memory/table operations until implemented as a full vertical slice;
- SIMD;
- memory64;
- multi-memory/multi-table;
- JIT compilation.
