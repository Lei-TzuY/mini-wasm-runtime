# Architecture

`mini-wasm-runtime` separates decoding, validation, instantiation, and execution so each layer has an explicit trust boundary.

## Data flow

```text
.wasm bytes
   |
   v
wasm-parser  ---- rejects malformed/truncated/unsupported binary structure
   |
   v
Module AST (types/functions/memory/exports/code/data)
   |
   v
wasm-validator ---- proves supported index, stack, control, and memory invariants
   |
   v
validated Module
   |
   v
Instance ---- allocates initial memory + applies active data segments
   |
   v
wasm-runtime ---- precomputes control boundaries, then interprets checked instructions
   |
   v
Value / trap-like RuntimeError
```

The CLI is deliberately thin and does not own parsing, validation, instantiation, or execution semantics.

## `wasm-parser`

The parser consumes raw bytes with a bounds-checked cursor. Lengths are checked before slices are formed. The current decoder admits type (1), function (3), memory (5), export (7), code (10), and data (11) sections. Custom sections are skipped; other standard sections are rejected explicitly.

Memory limits decode a minimum and optional maximum. Phase 3 data support is deliberately narrow: active segments targeting memory 0 with an `i32.const ... end` offset expression. Passive segments, explicit nonzero-memory active segments, and broader constant expressions are rejected rather than approximated.

LEB128 decoding is centralized and reused by the validator and interpreter for instruction immediates.

## `wasm-validator`

Validation combines cross-section checks with a typed instruction-stream pass. Because the current executable value domain is intentionally i32-only, value typing can be represented by stack arity plus the known i32 type.

The validator enforces:

- function declarations and code bodies have equal cardinality;
- every defined function references an existing type;
- export names are unique and function/memory export indices exist;
- function parameters, results, and locals are i32-only;
- functions and structured blocks expose at most one result;
- local and direct-call indices exist;
- instruction immediates are well formed;
- arithmetic, locals, calls, conditions, returns, and memory instructions have sufficient operand-stack values;
- `block`, `loop`, and `if` push explicit control frames with entry stack heights;
- `else` and `end` require arm/frame stack convergence;
- branch depths resolve to existing labels;
- block/if labels carry block results while loop labels restart with zero result values in the current no-block-parameters subset;
- unconditional transfer uses WebAssembly-style unreachable stack polymorphism while dead bytes remain opcode-checked;
- at most one linear memory is declared;
- memory minimum/maximum ordering is valid and both limits remain within 65,536 pages;
- memory operations require an existing memory;
- load/store alignment exponents do not exceed each opcode's natural alignment;
- `memory.size` / `memory.grow` memory indices exist;
- active data segments target an existing memory.

Supported block types are currently empty (`0x40`) and single-i32-result (`0x7f`). Block parameters and type-index block signatures are deferred.

## `wasm-runtime`

Each `Instance` owns the validated module, precomputed function control maps, and an optional `LinearMemory`. Each function invocation owns argument/local slots, an operand stack, a program counter, and an execution control stack.

### Structured control

Before invocation, the runtime builds a `ControlMap` for each function that pairs structured-control openers with their `else`/`end` boundaries. Runtime branches therefore do not rescan the byte stream to rediscover nesting.

Execution includes block/loop/if frame entry/exit, if/else path selection, depth-based branches, label-result preservation, loop back-edges, function returns, recursive direct calls with a depth limit, and wrapping i32 arithmetic.

### Linear memory

A WebAssembly page is 65,536 bytes. Instantiation allocates the declared minimum pages using fallible reservation and zero-initializes them. Active data segments are copied only after checking that the entire segment fits the initial memory; otherwise instantiation fails.

Memory addresses are 32-bit values. The runtime interprets the i32 address bits as an unsigned u32 and computes the effective byte address as:

```text
effective_address = u32(address) + memarg.offset
```

The computation is widened before bounds checks. Every load/store validates the complete accessed byte range before reading or writing, so accesses crossing the end of memory fail with `MemoryOutOfBounds` rather than partially touching memory.

Multi-byte values use WebAssembly little-endian encoding. Phase 3 implements i32 32-bit loads/stores plus 8/16-bit signed/unsigned loads and narrow stores.

`memory.grow` is deliberately distinct from an access trap: it returns the previous page count on success and `-1` if the requested growth would overflow, exceed the declared maximum/spec maximum, or cannot be allocated. Newly grown memory is zero initialized.

Runtime stack, index, control-boundary, memory-boundary, and result-arity checks remain as defense in depth even when validation has already established related static invariants.

## Current non-goals

- imports or host functions
- tables or indirect calls
- globals
- multiple memories / memory64 / shared memory
- passive or explicit-memory-index data segments
- block parameters or type-index block signatures
- i64/f32/f64 execution
- complete WebAssembly comparison/test instruction coverage
- complete spec-test conformance
- JIT compilation
- production sandboxing

These omissions are explicit so unsupported behavior cannot accidentally look valid.
