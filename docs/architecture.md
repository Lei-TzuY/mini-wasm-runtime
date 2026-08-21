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
wasm-validator ---- checks cross-section + instruction-stream invariants
   |
   v
validated Module
   |
   v
wasm-runtime ---- interprets supported instructions with checked stacks/locals
   |
   v
Value / runtime error
```

The CLI is deliberately thin and does not own parsing or execution semantics.

## `wasm-parser`

The parser consumes raw bytes with a bounds-checked cursor. Lengths are checked before slices are formed. Phase 1 decodes only the sections needed for a callable integer function: type (1), function (3), export (7), and code (10). Custom sections are skipped. Other standard sections are rejected explicitly.

LEB128 decoding is centralized and reused by the validator and interpreter for instruction immediates.

## `wasm-validator`

Validation enforces structural relationships that cannot be checked while decoding one section in isolation:

- function declarations and code bodies have equal cardinality;
- every defined function references an existing type;
- every exported function index exists;
- export names are unique;
- Phase-1 exports are functions;
- local counts cannot overflow the host index type;
- local instructions reference existing locals;
- direct calls reference existing functions;
- instruction immediates are well formed;
- only the Phase-1 opcode subset is admitted, including unreachable bytes after `return`;
- the outer `end` is present exactly at the end of each function body.

This instruction-stream pass is intentionally distinct from full WebAssembly type validation. Operand-stack typing, control-stack typing, polymorphic unreachable rules, and structured control-flow validation remain Phase 2 work. Runtime stack/arity checks remain as defense in depth.

## `wasm-runtime`

The runtime is a small stack machine. Each invocation owns:

- argument/local slots;
- an operand stack;
- a program counter over the function body.

Integer arithmetic uses Rust wrapping operations to match WebAssembly i32 overflow semantics. Calls are recursively interpreted with a depth limit. Unsupported types, stack underflow, bad locals, and result arity mismatches are runtime errors. Unknown opcodes are rejected by validation and remain guarded in the interpreter as a defensive fallback.

## Non-goals for Phase 1

- imports or host functions
- tables or indirect calls
- linear memory
- globals
- structured control flow (`block`, `loop`, `if`, `br`)
- floating-point execution
- full spec validation
- JIT compilation
- production sandboxing

These omissions are explicit so unsupported behavior cannot accidentally look valid.
