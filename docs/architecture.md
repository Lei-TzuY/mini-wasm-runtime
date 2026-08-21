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
wasm-validator ---- proves typed stack, index, signature, control, state, and memory invariants
   |
   v
validated Module + explicit HostRegistry + RuntimeLimits
   |
   v
Instance ---- resolves imports; allocates table/memory/globals; applies segments; runs start
   |
   v
wasm-runtime ---- typed numeric interpreter / scoped host callbacks / trap checks
   |
   v
Value / trap-like RuntimeError
```

The CLI is deliberately thin and does not own parsing, validation, instantiation, host capability, or execution semantics.

## `wasm-parser`

The parser consumes raw bytes with a bounds-checked cursor. Lengths are checked before slices are formed. Standard sections 1 through 11 currently cover type, import, function, table, memory, global, export, start, element, code, and data. Custom sections remain skippable.

Imports remain function-only. Tables are defined `funcref` tables. Defined global initializers accept the current MVP numeric constant-expression subset: `i32.const`, `i64.const`, `f32.const`, or `f64.const`, followed by `end`; the initializer's type must exactly equal the declared global type. Float constants are stored as raw `u32`/`u64` IEEE-754 bit patterns in the parsed `Constant` enum so parsing does not normalize NaN payloads.

Active element support remains deliberately narrow: mode 0, table 0, an `i32.const ... end` offset, and function-index payloads. Data segments retain the analogous active memory-0 restriction. Unsupported import kinds, reference types, segment modes, constant-expression forms, and malformed lengths fail explicitly.

LEB128 decoding is centralized for u32, i32, and i64 instruction/data immediates.

## `wasm-validator`

Phase 5B makes the typed validator the sole instruction-validation model. The earlier arity-only i32 validator was intentionally removed after differential validation and the expanded test suite were green.

### Typed operand and control stacks

The operand stack stores one `ValueType` for every reachable value. Defined code may use `I32`, `I64`, `F32`, or `F64`. A control frame stores:

- its kind (`function`, `block`, `loop`, `if`);
- the operand-stack height at entry;
- its optional result type;
- its optional label type;
- unreachable/polymorphic state;
- whether an `if` has crossed an `else` boundary.

Every instruction is validated as an exact typed stack transform. Locals and globals use their declared type. Direct calls consume the target function's parameter types in reverse stack order and push its optional result. `call_indirect` additionally validates the table/type indices and applies the selected function type's exact static stack effect. Conditions, table selectors, memory addresses, and current memory values remain `i32` where required by the supported subset.

Structured-control convergence checks both stack height and result type. Branches preserve the target label's exact result type. Loop labels currently have no parameter values because block parameters/type-index block signatures remain out of scope.

Unreachable code follows WebAssembly-style stack polymorphism: once control is unreachable, a pop at the current frame's base height can satisfy an instruction without inventing a concrete value, while concrete values that are present above that base must still have the required type. Dead bytes remain opcode/immediate checked.

### Cross-section invariants

Validation also enforces:

- defined function declarations and code bodies have equal cardinality;
- imported and defined function indices share the WebAssembly function index space;
- defined functions have zero or one numeric result;
- function imports remain i32-only with zero or one result in Phase 5B;
- table/global/function/memory export indices exist and export names are unique;
- at most one defined `funcref` table with valid min/max ordering;
- global indices exist and `global.set` targets mutable globals;
- start function exists and has exact signature `[] -> []`;
- active element segments target the existing table and valid function indices;
- active data segments target the existing memory;
- at most one linear memory, with valid page limits/alignment/index immediates.

Supported immediate block result types are empty, i32, i64, f32, and f64. Block parameters, type-index block signatures, and multi-value results are deferred.

## `wasm-runtime`

Each `Instance` owns the validated module, precomputed control maps, optional `LinearMemory`, optional function table, typed global values, resolved `HostRegistry`, and `RuntimeLimits`.

### Numeric value model

`Value` is a four-variant runtime enum:

```text
I32(i32)
I64(i64)
F32(f32)
F64(f64)
```

Locals are zero-initialized according to their declared type. Function invocation checks every supplied argument's runtime variant against the declared parameter type before executing code. Runtime stack helpers pop an expected type and return `ValueTypeMismatch` on disagreement, providing defense in depth behind static validation.

Numeric constants execute as their native runtime type. i32/i64 addition, subtraction, and multiplication use wrapping integer semantics. f32/f64 add/sub/mul/div use native IEEE-754 arithmetic. Integer comparisons distinguish signed and unsigned variants; float comparisons follow IEEE behavior, including unordered NaN comparisons. The current conversion slice implements `i32.wrap_i64`, signed/unsigned i64 extension from i32, f64-to-f32 demotion, and f32-to-f64 promotion.

Trapping float-to-integer conversions, reinterpret operations, and broader integer/floating operators remain intentionally unsupported rather than approximated.

### Typed control execution

Precomputed `ControlMap` metadata stores the optional result type for each structured-control opener. Active execution frames repeat that result type, so frame exit and branch unwinding check both the expected number of values and their runtime variant. This mirrors the typed validator rather than reverting to an arity-only runtime assumption.

### Globals

Defined globals are instantiated from typed parsed constants. `global.get` returns the current typed value. `global.set` repeats index, mutability, and runtime value-type checks before mutation. State persists across exported invocations.

### Tables, elements, and `call_indirect`

The table representation is `Vec<Option<u32>>`: each slot is null/uninitialized or a function index. Allocation uses fallible reservation. Active element segments are applied only after whole-range bounds checks.

`call_indirect` requires an i32 table selector and distinguishes:

- selector outside the table -> `TableElementOutOfBounds`;
- in-bounds null slot -> `UninitializedTableElement`;
- populated slot whose actual function type differs from the declared call-site type -> `IndirectCallTypeMismatch`.

After that dynamic signature check, calls use the same imported/defined dispatch path as direct `call`. Defined indirect targets may have any currently supported numeric signature.

### Host binding and capability boundary

`HostRegistry` remains deliberately i32-only in Phase 5B. This keeps the external embedding ABI stable while the internal WebAssembly numeric model expands. Instantiation rejects non-i32 import signatures. Host calls validate runtime argument variants before invoking the callback, so malformed embedding input cannot reach a callback through an i32 declaration.

A host callback receives `HostContext`, not `Instance`; memory access requires explicit `NONE`, `MEMORY_READ`, or `MEMORY_READ_WRITE` capabilities.

### Start function and resource limits

After imports are resolved and memory/globals/data/table/elements are initialized, an optional validated `[] -> []` start function executes automatically. Start execution uses the same call-depth, instruction-fuel, and host-call limits as ordinary execution.

`RuntimeLimits` independently controls maximum WebAssembly call depth, maximum memory pages, optional per-export instruction fuel, and optional per-export host-call count.

### Linear memory

Linear memory remains intentionally focused on the existing i32 family: 64-KiB pages, widened checked effective addresses, little-endian i32/narrow loads and stores, fallible growth, and whole-range data initialization. Phase 5B does not imply i64/f32/f64 memory opcodes.

Runtime signature, value-type, stack, control-boundary, memory/table boundary, global mutability, capability, indirect-type, and result checks remain as defense in depth even after validation.

## Current non-goals

- table, memory, or global imports
- non-i32 host function imports/callback signatures
- multiple tables or memories
- passive/declarative/explicit-table element modes
- passive or explicit-memory-index data segments
- block parameters or type-index block signatures
- multi-value results
- i64/f32/f64 memory load/store families
- trapping float-to-integer conversions and reinterpret instructions
- complete WebAssembly numeric instruction coverage
- complete spec-test conformance
- host re-entrancy into the same `Instance`
- JIT compilation
- production sandboxing

These omissions are explicit so unsupported behavior cannot accidentally look valid.
