# Architecture

`mini-wasm-runtime` separates decoding, structural validation, and execution so each layer has an explicit trust boundary.

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
wasm-validator ---- checks cross-section indices and invariants
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

LEB128 decoding is centralized and reused by the interpreter for instruction immediates.

## `wasm-validator`

Validation currently enforces structural relationships that cannot be checked while decoding one section in isolation:

- function declarations and code bodies have equal cardinality;
- every defined function references an existing type;
- every exported function index exists;
- export names are unique;
- Phase-1 exports are functions;
- local counts cannot overflow the host index type.

Opcode-level type validation is intentionally deferred to a later phase. Until then, execution fails closed when stack/arity invariants are violated.

## `wasm-runtime`

The runtime is a small stack machine. Each invocation owns:

- argument/local slots;
- an operand stack;
- a program counter over the function body.

Integer arithmetic uses Rust wrapping operations to match WebAssembly i32 overflow semantics. Calls are recursively interpreted with a depth limit. Unknown opcodes, unsupported types, stack underflow, bad locals, and result arity mismatches are runtime errors.

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
