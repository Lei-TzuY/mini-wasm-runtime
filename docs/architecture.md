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
Module AST (types/import descriptors/functions/tables/memory/globals/start/elements/code/data)
   |
   v
wasm-validator ---- proves typed stack, control, signature, and independent index-space invariants
   |
   v
validated Module + explicit HostRegistry + RuntimeLimits
   |
   v
Instance ---- resolves supported imports; allocates owned state; applies segments; runs start
   |
   v
wasm-runtime ---- typed numeric interpreter / scoped host callbacks / trap checks
   |
   v
Value / trap-like RuntimeError
```

The CLI is deliberately thin and does not own parsing, validation, instantiation, host capability, or execution semantics.

## `wasm-parser`

The parser consumes raw bytes with a bounds-checked cursor. Standard sections 1 through 11 currently cover type, import, function, table, memory, global, export, start, element, code, and data. Custom sections remain skippable.

### Import descriptors and index spaces

Imports retain binary-section order, but each import carries an explicit descriptor:

```text
Function(type_index)
Table(TableType)
Memory(MemoryType)
Global(GlobalType)
```

`Module` exposes separate function/table/memory/global import counts and lookup helpers. Imported objects precede defined objects only inside their own WebAssembly index space. A memory or global import therefore never shifts a function index.

Tables use the current `funcref` subset. Global imports carry their value type and mutability. Defined global initializers accept the current MVP numeric constant-expression subset: `i32.const`, `i64.const`, `f32.const`, or `f64.const`, followed by `end`; the initializer type must exactly equal the declared global type. Float constants are retained as raw IEEE-754 bits in the parsed `Constant` enum.

Active element support remains mode 0 / table 0 with an i32 constant offset and function indices. Data segments retain the analogous active memory-0 restriction. Unsupported reference types, segment modes, constant-expression forms, or malformed encodings fail explicitly.

LEB128 decoding includes u32, i32, i64, and signed-33 decoding for type-index block signatures.

## `wasm-validator`

The typed validator is the sole executable instruction-validation model.

### Typed operand and control stacks

The operand stack stores one `ValueType` per reachable value. Defined code may use `I32`, `I64`, `F32`, or `F64`. Control frames track:

- control kind (`function`, `block`, `loop`, `if`);
- operand-stack height at entry;
- block parameter types;
- optional result type;
- exact label types;
- unreachable/polymorphic state;
- whether an `if` has crossed an `else` boundary.

Every instruction is validated as an exact typed stack transform. Locals and globals use declared types. Direct calls consume target parameters and push the optional result. `call_indirect` validates table/type indices before applying the selected signature.

### Type-index block signatures

Immediate block types still support empty and single numeric value types. Phase 5C additionally decodes signed-33 type indices. The referenced function type may contribute multiple parameters but currently at most one result.

At control entry, parameter values are consumed and reintroduced as the new frame's parameter stack. For block/if labels, branch values are the block result types. For loop labels, branch values are the loop parameter types. `else` restores the block parameters so each arm starts from the same typed state.

Multi-result block signatures remain fail-closed.

### Independent cross-section index spaces

Validation treats functions, tables, memories, and globals independently. Imported objects precede defined objects in the corresponding kind only.

Validation currently enforces, among other invariants:

- defined function declarations and code bodies have equal cardinality;
- function imports plus defined functions form the function index space;
- table imports plus defined tables form the table index space;
- memory imports plus defined memories form the memory index space;
- global imports plus defined globals form the global index space;
- export indices resolve in the matching kind and export names are unique;
- function imports remain i32-only with zero or one result;
- defined functions have zero or one numeric result;
- at most one total table and one total memory are accepted by the current runtime subset;
- memory/table limits are validated even when the object is imported;
- `global.get` / `global.set` use the combined global index space and exact mutability/type information;
- start functions have exact signature `[] -> []`;
- active element/data segments target existing objects and valid indices.

Unreachable code follows WebAssembly-style stack polymorphism while still checking concrete opcodes and immediates.

## `wasm-runtime`

Each `Instance` owns the validated module, precomputed control maps, optional owned `LinearMemory`, optional owned function table, a combined global value vector, resolved `HostRegistry`, and `RuntimeLimits`.

### Numeric value model

`Value` is a four-variant runtime enum:

```text
I32(i32)
I64(i64)
F32(f32)
F64(f64)
```

Locals are zero-initialized by declared type. Invocation checks argument variants before execution. Runtime stack helpers preserve type checks as defense in depth behind validation.

The numeric slice includes i32/i64 wrapping add/sub/mul, f32/f64 add/sub/mul/div, integer signed/unsigned comparisons, IEEE floating comparisons, and selected non-trapping conversions.

### Typed control execution

`ControlMap` stores each structured opener's full supported block signature. Runtime frames retain block parameters and optional result types. Entry, frame exit, branch unwinding, loop backedges, and if/else transitions repeat the validator's typed invariants instead of reverting to arity-only assumptions.

### Globals and immutable imported globals

Runtime global storage uses the WebAssembly global index order:

```text
[ imported globals ... ][ defined globals ... ]
```

Defined globals are initialized from parsed constants. Immutable global imports are bound explicitly through `HostRegistry::register_immutable_global(module, name, value)` before instantiation. Binding requires exact numeric `ValueType` equality; missing bindings and type mismatches are distinct runtime errors.

`global.get` indexes the combined runtime vector. `global.set` obtains mutability and value type through the combined module global index space, so imported globals cannot shift defined-global mutation targets.

Mutable global imports remain rejected. Copying the initial host value into the instance would violate observable WebAssembly aliasing semantics, so support waits for a shared backing abstraction.

### Tables, elements, and `call_indirect`

The currently owned table representation is `Vec<Option<u32>>`. Active element segments are applied after whole-range bounds checks.

`call_indirect` distinguishes out-of-bounds selectors, null entries, and dynamic type mismatches before dispatching through the same imported/defined function path as direct calls.

Table imports are parsed and validated in the correct index space but remain rejected at instantiation until shared backing semantics are available.

### Linear memory

The owned linear-memory implementation uses 64-KiB pages, widened effective-address checks, little-endian i32/narrow loads and stores, fallible growth, and whole-range data initialization.

Memory imports participate in validation and the memory index space, but instantiation still rejects them. A copy of host bytes would not preserve imported-memory identity, growth visibility, or mutation aliasing.

### Host binding and capability boundary

`HostRegistry` currently holds two executable binding classes:

1. host functions, with i32-only signatures and zero-or-one result;
2. immutable numeric globals, supporting all four current numeric `Value` variants.

Host functions receive a `HostContext`, not the `Instance`. Memory access requires explicit `NONE`, `MEMORY_READ`, or `MEMORY_READ_WRITE` capabilities. Runtime arguments are type-checked before callbacks run.

The standalone CLI registers no implicit bindings or capabilities.

### Start function and resource limits

After supported imports are resolved and state/segments are initialized, an optional validated `[] -> []` start function executes automatically. Start execution uses the same call-depth, instruction-fuel, and host-call accounting as exported execution.

`RuntimeLimits` independently controls maximum WebAssembly call depth, maximum memory pages, optional instruction fuel, and optional host-call count.

## Current non-goals

- mutable global imports until shared backing exists
- table imports until shared backing exists
- memory imports until shared backing exists
- non-i32 host function callbacks
- multiple tables or memories
- passive/declarative element modes
- passive or explicit-memory-index data modes
- multi-value results
- i64/f32/f64 memory load/store families
- trapping float-to-integer conversions and reinterpret instructions
- complete numeric opcode coverage
- complete WebAssembly spec-test conformance
- host re-entrancy into the same `Instance`
- JIT compilation
- production sandboxing

These omissions are explicit so unsupported behavior cannot accidentally look valid.
