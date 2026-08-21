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
Module AST (types/imports/functions/memory/exports/code/data)
   |
   v
wasm-validator ---- proves supported index, signature, stack, control, and memory invariants
   |
   v
validated Module + explicit HostRegistry + RuntimeLimits
   |
   v
Instance ---- resolves imports, allocates memory, applies active data segments
   |
   v
wasm-runtime ---- meters and interprets checked instructions / invokes scoped host callbacks
   |
   v
Value / trap-like RuntimeError
```

The CLI is deliberately thin and does not own parsing, validation, instantiation, host capability, or execution semantics.

## `wasm-parser`

The parser consumes raw bytes with a bounds-checked cursor. Lengths are checked before slices are formed. The current decoder admits type (1), import (2), function (3), memory (5), export (7), code (10), and data (11) sections. Custom sections are skipped.

Phase 4 accepts only function imports. Each import records its module name, field name, and function type index. Table, memory, and global imports are rejected explicitly rather than partially decoded.

Memory limits decode a minimum and optional maximum. Active data support remains deliberately narrow: memory 0 with an `i32.const ... end` offset expression. Passive segments, explicit nonzero-memory active segments, and broader constant expressions remain unsupported.

LEB128 decoding is centralized and reused by the validator and interpreter for instruction immediates.

## `wasm-validator`

Validation combines cross-section checks with a typed instruction-stream pass. The current executable value domain is intentionally i32-only.

### Function index space

WebAssembly places imported functions before defined functions in one function index space:

```text
0 .. imported_function_count                  imported functions
imported_function_count .. total_function_count  defined functions
```

Code bodies still correspond only to defined functions. Validation preserves that distinction while resolving function exports and `call` targets against the combined index space.

The validator enforces:

- each function import references an existing function type;
- imported and defined function signatures are i32-only with at most one result;
- function declarations and code bodies have equal cardinality for defined functions;
- every defined function references an existing type;
- export names are unique and function/memory export indices exist;
- local and direct-call indices exist;
- calls to imported and defined functions apply the target signature's stack effect;
- instruction immediates are well formed;
- arithmetic, locals, calls, conditions, returns, and memory instructions have sufficient operand-stack values;
- structured control frames converge correctly and branch depths/label arities are valid;
- unconditional transfer uses WebAssembly-style unreachable stack polymorphism while dead bytes remain opcode-checked;
- at most one linear memory is declared;
- memory minimum/maximum ordering is valid and both limits remain within 65,536 pages;
- memory operations require an existing memory and valid alignment/index immediates;
- active data segments target an existing memory.

Supported block types remain empty (`0x40`) and single-i32-result (`0x7f`). Block parameters and type-index block signatures are deferred.

## `wasm-runtime`

Each `Instance` owns the validated module, precomputed function control maps, optional `LinearMemory`, a resolved `HostRegistry`, and `RuntimeLimits`. Each defined-function invocation owns argument/local slots, an operand stack, a program counter, and an execution control stack.

### Host binding

`HostRegistry` binds a function by `(module, name)` and declares the exact parameter/result signature expected by that callback. Instantiation fails if any module import is unresolved or if the registered signature does not exactly match the WebAssembly import type.

Imported functions occupy their WebAssembly function indices. `invoke_function` dispatches an imported index to the host registry and a defined index to the corresponding code body. This means a normal `call` instruction requires no special host opcode.

Host callbacks return zero or one `Value::I32` in the current subset. Runtime code checks the returned arity after the callback rather than trusting the embedding blindly.

### Capability boundary

A host callback receives `HostContext`, not `Instance`. This deliberately prevents a callback from reaching arbitrary VM internals or recursively invoking exports through an unrestricted handle.

Memory authority is explicit at registration:

- `HostCapabilities::NONE`: no linear-memory access;
- `HostCapabilities::MEMORY_READ`: bounded reads and memory-size inspection;
- `HostCapabilities::MEMORY_READ_WRITE`: bounded reads plus writes.

`HostContext::read_memory` returns a copy of the requested bytes. Reads and writes check the entire range before access and fail if the capability was not granted, no memory exists, or the range is outside memory.

### Runtime resource limits

`RuntimeLimits` provides embedding-controlled ceilings independent of the module's own declarations:

- `max_call_depth`: bounds recursive/inter-function WASM execution;
- `max_memory_pages`: caps initial allocation and future `memory.grow` even when the module declares a larger maximum;
- `fuel`: optional instruction budget reset for every exported invocation;
- `max_host_calls`: optional host-call budget reset for every exported invocation.

Fuel is consumed by interpreted WebAssembly instructions. Host calls are metered separately, making the two resources observable and independently configurable.

### Structured control

Before invocation, the runtime builds a `ControlMap` for every defined function that pairs structured-control openers with their `else`/`end` boundaries. Runtime branches therefore do not rescan nesting on every jump.

Execution includes block/loop/if frame entry/exit, if/else path selection, depth-based branches, label-result preservation, loop back-edges, returns, direct calls across the imported/defined function index space, and wrapping i32 arithmetic.

### Linear memory

A WebAssembly page is 65,536 bytes. Instantiation allocates the declared minimum using fallible reservation and zero initialization. The effective maximum is the tighter of the module's declared maximum, the WebAssembly 65,536-page bound, and the embedding's `max_memory_pages` limit.

Active data segments are copied only after their complete ranges are checked. Memory instruction addresses treat i32 bits as unsigned u32 addresses, widen effective-address arithmetic, and validate the complete accessed byte range before touching memory. Multi-byte values use WebAssembly little-endian encoding.

`memory.grow` returns the previous page count on success and `-1` when growth would overflow, exceed the effective maximum, or fail allocation. Access violations remain trap-like `RuntimeError`s rather than `memory.grow` failures.

Runtime signature, stack, control-boundary, memory-boundary, capability, and result-arity checks remain as defense in depth even when validation has already established related static invariants.

## Current non-goals

- table, memory, or global imports
- tables or `call_indirect`
- globals
- multiple memories / memory64 / shared memory
- passive or explicit-memory-index data segments
- block parameters or type-index block signatures
- i64/f32/f64 execution
- complete WebAssembly comparison/test instruction coverage
- complete spec-test conformance
- host re-entrancy into the same `Instance`
- JIT compilation
- production sandboxing

These omissions are explicit so unsupported behavior cannot accidentally look valid.
