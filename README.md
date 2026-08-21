# mini-wasm-runtime

A small WebAssembly runtime built from first principles in Rust.

The project intentionally does **not** embed Wasmtime/Wasmer or another WebAssembly engine. The parser, validator, interpreter, linear memory, and host boundary live in this repository so the interesting invariants stay visible and testable.

## Current milestone: Phase 4

Phase 4 extends the typed interpreter with explicit function imports and a capability-scoped host boundary:

- WebAssembly binary header plus u32/i32 LEB128 decoding
- type, function-import, function, memory, export, code, and active data sections
- combined function index space: imported functions first, defined functions after them
- cross-section, typed operand-stack, control-stack, import, and memory validation
- structured `block`, `loop`, `if`, `else`, `br`, and `br_if`
- one 32-bit linear memory with 64 KiB pages
- little-endian i32 load/store families, bounds traps, `memory.size`, and `memory.grow`
- active memory-0 data-segment initialization
- typed `HostRegistry` resolution by `(module, name)`
- capability-scoped `HostContext` instead of exposing the complete VM to callbacks
- explicit host memory-read / memory-write capabilities
- configurable call-depth, memory-page, instruction-fuel, and host-call limits
- direct calls may target imported or defined functions through the same validated index space
- CLI inspect/run commands and multi-platform CI plus Rust 1.81 MSRV validation

### Executable instruction subset

`block`, `loop`, `if`, `else`, `br`, `br_if`, `return`, `end`, `call`, `local.get`, `local.set`, `local.tee`, `i32.const`, `i32.add`, `i32.sub`, `i32.mul`, `i32.load`, `i32.load8_s`, `i32.load8_u`, `i32.load16_s`, `i32.load16_u`, `i32.store`, `i32.store8`, `i32.store16`, `memory.size`, and `memory.grow`.

Function, block, and host-call results are currently limited to zero or one `i32`. The runtime supports at most one linear memory. Phase 4 accepts function imports only; table, memory, and global imports remain fail-closed.

## Quick start

```bash
cargo test --workspace
cargo run -p wasm-cli -- inspect path/to/module.wasm
cargo run -p wasm-cli -- run path/to/module.wasm add 20 22
```

`mini-wasm inspect` reports imported functions and the combined function index space. The standalone CLI intentionally does **not** install implicit host functions or capabilities. Executing a module with imports therefore requires an embedding application to construct a `HostRegistry` and instantiate with `Instance::with_hosts` or `Instance::with_config`.

## Embedding host functions

```rust
use wasm_parser::ValueType;
use wasm_runtime::{HostCapabilities, HostRegistry, Instance, Value};

let mut hosts = HostRegistry::new();
hosts.register(
    "env",
    "double",
    vec![ValueType::I32],
    vec![ValueType::I32],
    HostCapabilities::NONE,
    |_ctx, args| Ok(Some(Value::I32(args[0].as_i32().wrapping_mul(2)))),
)?;

let mut instance = Instance::with_hosts(module, hosts)?;
```

Callbacks receive a `HostContext`, not the `Instance`. Memory access is denied unless the registration explicitly grants `MEMORY_READ` or `MEMORY_READ_WRITE`.

## Workspace

```text
crates/
  wasm-parser/     binary format + imports + memory/data sections
  wasm-validator/  index-space + typed control/import/memory invariants
  wasm-runtime/    interpreter + linear memory + capability-scoped host boundary
  wasm-cli/        inspect/run command-line frontend
docs/
  architecture.md
  roadmap.md
```

## Design principles

1. **Fail closed.** Unsupported binary features, import kinds, and opcodes are errors.
2. **No hidden engine dependency.** Runtime behavior is implemented here.
3. **Parser != validator != executor.** Each layer has a narrow contract.
4. **Validate before execute.** Indices, signatures, stack effects, control labels, memory limits, and alignment hints are checked before invocation.
5. **Least capability at the host boundary.** Host callbacks receive only explicitly granted facilities.
6. **Meter untrusted work.** Embedders may cap call depth, memory pages, instruction fuel, and host calls.
7. **Trap instead of corrupt.** Linear-memory ranges are checked before every access.
8. **Defense in depth.** Runtime checks remain even for invariants already proven by validation.
9. **Small vertical slices.** Every phase ends in executable behavior and tests.

See [`docs/architecture.md`](docs/architecture.md) for the current contracts and [`docs/roadmap.md`](docs/roadmap.md) for planned phases.

## Status

Experimental and intentionally incomplete. It is a learning/runtime-engineering project, not a production sandbox.

## License

MIT
