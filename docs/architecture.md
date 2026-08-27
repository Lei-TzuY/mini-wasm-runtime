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
Instance ---- resolves supported imports; attaches shared globals/tables/memory or owned defined state; applies segments; runs start
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

`Module` exposes separate function/table/memory/global import counts and lookup helpers. Imported objects precede defined objects only inside their own WebAssembly index space. A memory, table, or global import therefore never shifts a function index.

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

Every admitted instruction is validated as an exact typed stack transform. Locals and globals use declared types. Direct calls consume target parameters and push the optional result. `call_indirect` validates table/type indices before applying the selected signature. Untyped numeric `select` requires an i32 condition plus same-typed numeric candidates; `drop` consumes one polymorphic current-frame operand. Bit reinterpret instructions apply exact source/destination numeric types without numeric conversion.

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

Unreachable code follows WebAssembly-style stack polymorphism while still checking concrete opcodes and immediates. That validator state is distinct from the MVP `unreachable` instruction (`0x00`), which is still outside this Phase-5C branch's admitted opcode surface and therefore remains explicitly fail closed.

## `wasm-runtime`

Each `Instance` owns the validated module, a private instance identity, precomputed control maps, its resolved memory/table/global state, resolved `HostRegistry`, and `RuntimeLimits`. Imported mutable objects retain shared host-visible backing rather than being copied into shadow state.

### Numeric value model

`Value` is a four-variant runtime enum:

```text
I32(i32)
I64(i64)
F32(f32)
F64(f64)
```

Locals are zero-initialized by declared type. Invocation checks argument variants before execution. Runtime stack helpers preserve type checks as defense in depth behind validation.

The numeric slice includes i32/i64 wrapping add/sub/mul, f32/f64 add/sub/mul/div, integer signed/unsigned comparisons, IEEE floating comparisons, selected non-trapping conversions, and all four bit reinterpret directions. Reinterpret execution uses the exact 32/64-bit representation so NaN payloads and signed zero are preserved.

### Typed control and parametric execution

`ControlMap` stores each structured opener's full supported block signature. Runtime frames retain block parameters and optional result types. Entry, frame exit, branch unwinding, loop backedges, and if/else transitions repeat the validator's typed invariants instead of reverting to arity-only assumptions.

Untyped `select` pops an i32 condition and two same-typed numeric candidates and returns one candidate without numeric conversion. `drop` removes exactly one top value. Both are admitted across validator, runtime dispatch, and control-map scanning together. `nop`, MVP `unreachable`, `br_table`, and typed select remain outside the admitted surface on this stack until each corresponding vertical slice is complete.

### Globals and shared imported state

Runtime global storage follows WebAssembly global index order:

```text
[ imported global handles ... ][ defined global handles ... ]
```

All runtime global slots use `GlobalHandle`. A handle stores:

```text
Rc<RefCell<Value>> + ValueType + mutable flag
```

This matches the runtime's current single-threaded embedding model. It deliberately does not claim `Send`, `Sync`, threads, or shared-memory support.

Defined globals get private handles initialized from parsed constants. Imported globals use handles supplied by the embedding application:

- `HostRegistry::register_immutable_global(module, name, value)` creates an immutable handle;
- `HostRegistry::register_global(module, name, handle)` registers a retained `GlobalHandle`, including mutable handles.

Instantiation requires both the numeric `ValueType` and mutability flag to equal the imported `GlobalType`. Missing bindings, type mismatches, and mutability mismatches are distinct errors.

For mutable imports, the host and instance clone the same handle rather than copying its value. Therefore host writes are observed by later `global.get` instructions and WebAssembly `global.set` updates the same host-visible cell. The handle itself rejects writes to immutable globals and wrong-type writes.

### Tables, elements, and `call_indirect`

Defined and imported tables use the same `TableHandle` abstraction. A handle stores a shared vector of optional opaque `FunctionRef` values plus the table's optional maximum:

```text
Rc<RefCell<Vec<Option<FunctionRef>>>> + maximum + live-instance binding
```

An imported table is supplied through `HostRegistry::register_table`. Instantiation checks WebAssembly table-limit matching: the supplied current length must satisfy the import minimum, and when the import declares a maximum the supplied handle must also declare a maximum no larger than that import maximum.

`FunctionRef` does not expose a raw function index to the embedding API. Each reference carries a weak identity for the runtime instance that created it. Before `call_indirect` dispatches, the runtime verifies that the reference still belongs to the current live instance. This makes stale or foreign references fail as `ForeignTableFunctionReference` instead of accidentally invoking whatever function happens to reuse the same numeric index in another instance.

The current store model deliberately binds one `TableHandle` to at most one live runtime instance. Reusing it after that instance is dropped is allowed, but stale function references from the old instance remain invalid. Full cross-instance/reference-store semantics are deferred rather than approximated.

Active element initialization against an imported table is host-visible. To avoid leaking partial instantiation side effects, the runtime first preflights **all** active element ranges. Only after every range is valid are the segment writes applied to the shared handle. A later out-of-bounds segment therefore cannot leave earlier segment writes behind when instantiation fails.

Host mutations to table slots are immediately visible to `call_indirect`. Indirect calls distinguish:

- selector outside the table -> `TableElementOutOfBounds`;
- in-bounds null slot -> `UninitializedTableElement`;
- stale/foreign function reference -> `ForeignTableFunctionReference`;
- current-instance reference whose actual function type differs from the call-site type -> `IndirectCallTypeMismatch`.

### Linear memory

The linear-memory implementation uses 64-KiB pages, widened effective-address checks, little-endian i32/narrow loads and stores, fallible growth, and whole-range data initialization.

Defined memory is instance-owned. Imported memory is supplied through `HostRegistry::register_memory` as a retained `MemoryHandle`; the host and Wasm instance observe the same backing bytes, current page count, growth, and maximum. Import limit matching and `RuntimeLimits::max_memory_pages` are checked before instantiation.

Active data initialization is transactional for both owned and imported backing: every active segment range is preflighted before any payload is copied. A later failing segment therefore cannot leave an earlier partial host-visible mutation. Host byte writes are visible to later Wasm loads, Wasm stores are visible through the retained handle, and `memory.size` / `memory.grow` operate on the same imported backing.

For the supported i32 full-width and narrow load/store family, effective addresses widen the unsigned i32 base plus unsigned memarg offset before bounds checks, so 32-bit wraparound cannot turn an out-of-bounds address into an in-bounds access. Failed stores preflight the whole range before mutation.

### Host binding and capability boundary

`HostRegistry` currently holds four executable binding classes:

1. host functions, with i32-only signatures and zero-or-one result;
2. numeric `GlobalHandle` bindings, immutable or mutable, supporting all four current numeric `Value` variants;
3. `TableHandle` bindings for the current single-table `funcref` subset;
4. `MemoryHandle` bindings for the current single-memory subset.

Host functions receive a `HostContext`, not the `Instance`. Memory access requires explicit `NONE`, `MEMORY_READ`, or `MEMORY_READ_WRITE` capabilities. Runtime arguments are type-checked before callbacks run.

The standalone CLI registers no implicit bindings or capabilities.

### Start function and resource limits

After supported imports are resolved and state/segments are initialized, an optional validated `[] -> []` start function executes automatically. Start execution uses the same call-depth, instruction-fuel, and host-call accounting as exported execution.

`RuntimeLimits` independently controls maximum WebAssembly call depth, maximum memory pages, optional instruction fuel, and optional host-call count.

## Current non-goals

- multiple live instances sharing one `TableHandle`
- cross-instance function-reference dispatch
- thread-safe/shared-memory global, table, or memory handles
- non-i32 host function callbacks
- multiple tables or memories
- passive/declarative element modes
- passive or explicit-memory-index data modes
- multi-value results
- `nop`, MVP `unreachable`, `br_table`, and typed select until their complete vertical slices land
- i64/f32/f64 memory load/store families
- trapping float-to-integer and integer-to-float conversions
- complete numeric opcode coverage
- complete WebAssembly spec-test conformance
- host re-entrancy into the same `Instance`
- JIT compilation
- production sandboxing

These omissions are explicit so unsupported behavior cannot accidentally look valid.
