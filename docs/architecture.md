# Architecture

`mini-wasm-runtime` separates decoding, validation, instantiation, host binding, and execution so each layer has an explicit trust boundary.

## Data flow

```text
.wasm bytes
   |
   v
wasm-parser  ---- rejects malformed/truncated/unsupported binary structure
   |
   v
Module AST (types/imports/functions/tables/memory/globals/start/elements/code/data)
   |
   v
wasm-validator ---- proves supported index, signature, stack, control, state, and memory invariants
   |
   v
validated Module + explicit HostRegistry + RuntimeLimits
   |
   v
Instance ---- resolves imports; allocates table/memory/globals; applies segments; runs start
   |
   v
wasm-runtime ---- meters and interprets checked instructions / invokes scoped host callbacks
   |
   v
Value / trap-like RuntimeError
```

The CLI is deliberately thin and does not own parsing, validation, instantiation, host capability, or execution semantics.

## `wasm-parser`

The parser consumes raw bytes with a bounds-checked cursor. Lengths are checked before slices are formed. Phase 5A decodes standard sections 1 through 11: type, import, function, table, memory, global, export, start, element, code, and data. Custom sections remain skippable.

Imports remain function-only. Tables are defined `funcref` tables; globals are defined globals whose initializer is currently limited to `i32.const ... end`. Active element support is deliberately narrow: mode 0, table 0, an `i32.const ... end` offset, and function-index payloads. Data segments retain the analogous active memory-0 restriction.

Unsupported import kinds, reference types, element/data modes, mutability encodings, constant expressions, value types, and malformed lengths fail explicitly rather than being approximated.

LEB128 decoding is centralized and reused by the validator and interpreter for instruction immediates.

## `wasm-validator`

Validation combines cross-section checks with a typed instruction-stream pass. The current executable value domain remains intentionally i32-only.

### Function index space

WebAssembly places imported functions before defined functions in one function index space. Code bodies still correspond only to defined functions. Function exports, direct calls, element payloads, start functions, and table entries all resolve against the appropriate validated function/type indices.

### Phase 5A state invariants

The validator enforces:

- at most one defined `funcref` table and valid table min/max ordering;
- defined globals are i32-only;
- `global.get` indices exist;
- `global.set` indices exist and target mutable globals;
- table/global export indices exist;
- start function index exists and its exact signature is `[] -> []`;
- active element segments target the existing table and reference existing functions;
- `call_indirect` references an existing table and function type;
- indirect-call signatures remain i32-only with zero or one result;
- `call_indirect` consumes the table selector plus the selected type's parameters and produces that type's results.

All earlier invariants remain in force: function-import/defined index separation, code cardinality, typed locals/calls, structured-control convergence, branch labels, unreachable stack polymorphism, memory limits/alignment, data segments, and host-call signatures.

Supported block types remain empty (`0x40`) and single-i32-result (`0x7f`). Block parameters and type-index block signatures are deferred.

## `wasm-runtime`

Each `Instance` owns the validated module, precomputed function control maps, optional `LinearMemory`, optional function table, instantiated globals, resolved `HostRegistry`, and `RuntimeLimits`.

### Globals

Defined globals are instantiated from their validated i32 constant initializers. `global.get` reads the current value. `global.set` mutates only mutable globals; the runtime repeats the mutability/index checks as defense in depth. Global values persist across exported invocations.

### Tables, elements, and `call_indirect`

The current table representation is `Vec<Option<u32>>`: each slot is either null/uninitialized or a function index. Allocation uses fallible reservation. Active element segments are applied during instantiation only after the complete target range is checked against the table's initial size.

`call_indirect` pops an i32 table-element selector, interprets its bits as a u32 index, and distinguishes three runtime failure classes:

- selector outside the table -> `TableElementOutOfBounds`;
- in-bounds null slot -> `UninitializedTableElement`;
- populated slot whose actual function type differs from the call site's declared type -> `IndirectCallTypeMismatch`.

Only after the dynamic signature check does execution dispatch through the same imported/defined function machinery used by direct `call`.

### Start function

After imports are resolved and memory, globals, data segments, table, and element segments are initialized, an optional start function is invoked automatically. Validation guarantees `[] -> []`. Start execution uses the same call-depth, instruction-fuel, and host-call limits as ordinary execution, rather than bypassing resource metering during instantiation.

### Host binding and capability boundary

`HostRegistry` binds a function by `(module, name)` and declares the exact parameter/result signature expected by that callback. Instantiation fails if any function import is unresolved or mismatched. A host callback receives `HostContext`, not `Instance`; memory access requires explicit `NONE`, `MEMORY_READ`, or `MEMORY_READ_WRITE` capability presets.

### Runtime resource limits

`RuntimeLimits` independently controls maximum WASM call depth, maximum linear-memory pages, optional per-export instruction fuel, and optional per-export host-call count. `memory.grow` respects both module and embedding ceilings.

### Structured control and linear memory

Control maps precompute structured `block`/`loop`/`if` boundaries so branches do not rescan bytecode nesting. Linear memory retains 64-KiB pages, checked widened effective addresses, little-endian loads/stores, fallible growth, and whole-range data initialization.

Runtime signature, stack, control-boundary, memory/table boundaries, global mutability, capability, indirect-type, and result-arity checks remain as defense in depth even when validation has already established related static invariants.

## Current non-goals

- table, memory, or global imports
- multiple tables or memories
- passive/declarative/explicit-table element modes
- passive or explicit-memory-index data segments
- block parameters or type-index block signatures
- i64/f32/f64 execution
- complete WebAssembly comparison/test/conversion instruction coverage
- complete spec-test conformance
- host re-entrancy into the same `Instance`
- JIT compilation
- production sandboxing

These omissions are explicit so unsupported behavior cannot accidentally look valid.
