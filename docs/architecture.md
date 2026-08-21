# Architecture

`mini-wasm-runtime` separates decoding, validation, and execution so each layer has an explicit trust boundary.

## Data flow

```text
.wasm bytes
   |
   v
wasm-parser  ---- rejects malformed/truncated/unsupported binary structure
   |
   v
Module AST
   |
   v
wasm-validator ---- proves supported index, operand-stack, and control-stack invariants
   |
   v
validated Module
   |
   v
wasm-runtime ---- precomputes control boundaries, then interprets checked instructions
   |
   v
Value / runtime error
```

The CLI is deliberately thin and does not own parsing, validation, or execution semantics.

## `wasm-parser`

The parser consumes raw bytes with a bounds-checked cursor. Lengths are checked before slices are formed. The current decoder admits only the sections needed for a callable integer function: type (1), function (3), export (7), and code (10). Custom sections are skipped. Other standard sections are rejected explicitly.

LEB128 decoding is centralized and reused by the validator and interpreter for instruction immediates.

## `wasm-validator`

Validation combines cross-section checks with a typed instruction-stream pass. Because the current executable value domain is intentionally i32-only, value typing can be represented by stack arity plus the known i32 type.

The validator enforces:

- function declarations and code bodies have equal cardinality;
- every defined function references an existing type;
- every exported function index exists and export names are unique;
- function parameters, results, and locals are i32-only;
- functions and structured blocks expose at most one result;
- local and direct-call indices exist;
- instruction immediates are well formed;
- arithmetic, locals, calls, conditions, and returns have sufficient operand-stack values;
- `block`, `loop`, and `if` push explicit control frames with entry stack heights;
- `else` and `end` require arm/frame stack convergence;
- branch depths resolve to existing labels;
- block/if labels carry block results while loop labels restart with zero result values in the current no-block-parameters subset;
- `return` targets the implicit function control label;
- unconditional transfer marks the current frame unreachable, allowing WebAssembly-style polymorphic stack behavior while still decoding and rejecting unsupported dead opcodes.

Supported block types are currently empty (`0x40`) and single-i32-result (`0x7f`). Block parameters and type-index block signatures are deferred.

## `wasm-runtime`

Each invocation owns argument/local slots, an operand stack, a program counter, and an execution control stack.

Before invocation, the runtime builds a `ControlMap` for each function that pairs structured-control openers with their `else`/`end` boundaries. Runtime branches therefore do not rescan the byte stream to rediscover nesting.

Execution semantics include:

- block/loop/if frame entry and exit;
- if/else path selection;
- depth-based `br` and `br_if`;
- label-result preservation while unwinding nested frames;
- loop back-edges to the loop body header;
- `return` through the implicit function label;
- direct recursive calls with a depth limit;
- wrapping i32 arithmetic.

Runtime stack, index, control-boundary, and result-arity checks remain as defense in depth even when validation has already established the corresponding invariant.

## Current non-goals

- imports or host functions
- tables or indirect calls
- linear memory
- globals
- block parameters or type-index block signatures
- i64/f32/f64 execution
- complete WebAssembly comparison/test instruction coverage
- complete spec-test conformance
- JIT compilation
- production sandboxing

These omissions are explicit so unsupported behavior cannot accidentally look valid.
