# mini-wasm-runtime

A small WebAssembly runtime built from first principles in Rust.

The project intentionally does **not** embed Wasmtime/Wasmer or another WebAssembly engine. The parser, structural validator, and interpreter are implemented in this repository so the interesting invariants stay visible and testable.

## Current milestone: Phase 1

Phase 1 implements a deliberately small but end-to-end slice of WebAssembly 1.0:

- binary module header parsing (`\0asm`, version 1)
- unsigned and signed LEB128 decoding
- type, function, export, and code sections
- structural validation across function/type/code/export indices
- a stack interpreter for an integer MVP subset
- CLI commands to inspect and execute modules
- unit/integration-style tests and GitHub Actions CI

### Executable instruction subset

`local.get`, `local.set`, `local.tee`, `i32.const`, `i32.add`, `i32.sub`, `i32.mul`, `call`, `return`, and `end`.

Unsupported standard sections and opcodes fail explicitly rather than being silently ignored.

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
  wasm-validator/  cross-section structural invariants
  wasm-runtime/    minimal stack interpreter
  wasm-cli/        inspect/run command-line frontend
docs/
  architecture.md
  roadmap.md
```

## Design principles

1. **Fail closed.** Unsupported binary features and opcodes are errors.
2. **No hidden engine dependency.** Runtime behavior is implemented here.
3. **Parser != validator != executor.** Each layer has a narrow contract.
4. **Bounds before trust.** Lengths and indices are checked before use.
5. **Small vertical slices.** Every phase should end in executable behavior and tests.

See [`docs/architecture.md`](docs/architecture.md) for the current contracts and [`docs/roadmap.md`](docs/roadmap.md) for planned phases.

## Status

Experimental and intentionally incomplete. It is a learning/runtime-engineering project, not a production sandbox.

## License

MIT
