# mini-wasm-runtime

A small WebAssembly runtime built from first principles in Rust.

The project intentionally does **not** embed Wasmtime/Wasmer or another WebAssembly engine. The parser, validator, and interpreter are implemented in this repository so the interesting invariants stay visible and testable.

## Current milestone: Phase 2

Phase 2 extends the callable integer MVP with typed structured control flow:

- WebAssembly binary header plus u32/i32 LEB128 decoding
- type, function, export, and code sections
- cross-section and instruction-stream validation
- operand-stack and control-stack validation for the supported i32 subset
- unreachable stack-polymorphism after unconditional transfer
- structured `block`, `loop`, `if`, and `else`
- depth-based `br` and `br_if` with label arity checks
- precomputed structured-control boundaries for execution
- direct calls, locals, constants, and wrapping i32 arithmetic
- CLI inspect/run commands
- multi-platform CI plus Rust 1.81 MSRV validation

### Executable instruction subset

`block`, `loop`, `if`, `else`, `br`, `br_if`, `return`, `end`, `call`, `local.get`, `local.set`, `local.tee`, `i32.const`, `i32.add`, `i32.sub`, and `i32.mul`.

Function and block results are currently limited to zero or one `i32`. Unsupported standard sections, block signatures, value types, and opcodes fail explicitly rather than being silently ignored.

## Quick start

```bash
cargo test --workspace
cargo run -p wasm-cli -- inspect path/to/module.wasm
cargo run -p wasm-cli -- run path/to/module.wasm add 20 22
```

## Workspace

```text
crates/
  wasm-parser/     binary format + LEB128
  wasm-validator/  cross-section + typed control-flow invariants
  wasm-runtime/    stack interpreter + structured control execution
  wasm-cli/        inspect/run command-line frontend
docs/
  architecture.md
  roadmap.md
```

## Design principles

1. **Fail closed.** Unsupported binary features and opcodes are errors.
2. **No hidden engine dependency.** Runtime behavior is implemented here.
3. **Parser != validator != executor.** Each layer has a narrow contract.
4. **Validate before execute.** Indices, immediates, stack effects, and control labels are checked before invocation.
5. **Defense in depth.** Runtime checks remain even for invariants already proven by validation.
6. **Small vertical slices.** Every phase ends in executable behavior and tests.

See [`docs/architecture.md`](docs/architecture.md) for the current contracts and [`docs/roadmap.md`](docs/roadmap.md) for planned phases.

## Status

Experimental and intentionally incomplete. It is a learning/runtime-engineering project, not a production sandbox.

## License

MIT
