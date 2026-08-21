# mini-wasm-runtime

A small WebAssembly runtime built from first principles in Rust.

The project intentionally does **not** embed Wasmtime/Wasmer or another WebAssembly engine. The parser, validator, interpreter, and linear-memory implementation live in this repository so the interesting invariants stay visible and testable.

## Current milestone: Phase 3

Phase 3 extends the typed structured-control runtime with a first linear-memory vertical slice:

- WebAssembly binary header plus u32/i32 LEB128 decoding
- type, function, memory, export, code, and active data sections
- cross-section, typed operand-stack, and control-stack validation
- structured `block`, `loop`, `if`, `else`, `br`, and `br_if`
- one 32-bit linear memory with 64 KiB pages
- validated memory min/max limits up to the WebAssembly 65,536-page bound
- little-endian i32 loads/stores, including 8/16-bit sign/zero-extending loads and narrow stores
- checked effective addresses with out-of-bounds traps
- `memory.size` and `memory.grow`
- active memory-0 data-segment initialization
- direct calls, locals, constants, and wrapping i32 arithmetic
- CLI inspect/run commands
- multi-platform CI plus Rust 1.81 MSRV validation

### Executable instruction subset

`block`, `loop`, `if`, `else`, `br`, `br_if`, `return`, `end`, `call`, `local.get`, `local.set`, `local.tee`, `i32.const`, `i32.add`, `i32.sub`, `i32.mul`, `i32.load`, `i32.load8_s`, `i32.load8_u`, `i32.load16_s`, `i32.load16_u`, `i32.store`, `i32.store8`, `i32.store16`, `memory.size`, and `memory.grow`.

Function and block results are currently limited to zero or one `i32`. The runtime currently supports at most one linear memory. Unsupported standard sections, data-segment modes, block signatures, value types, and opcodes fail explicitly rather than being silently ignored.

## Quick start

```bash
cargo test --workspace
cargo run -p wasm-cli -- inspect path/to/module.wasm
cargo run -p wasm-cli -- run path/to/module.wasm add 20 22
```

## Workspace

```text
crates/
  wasm-parser/     binary format + LEB128 + memory/data sections
  wasm-validator/  cross-section + typed control/memory invariants
  wasm-runtime/    stack interpreter + structured control + linear memory
  wasm-cli/        inspect/run command-line frontend
docs/
  architecture.md
  roadmap.md
```

## Design principles

1. **Fail closed.** Unsupported binary features and opcodes are errors.
2. **No hidden engine dependency.** Runtime behavior is implemented here.
3. **Parser != validator != executor.** Each layer has a narrow contract.
4. **Validate before execute.** Indices, immediates, stack effects, control labels, memory limits, and alignment hints are checked before invocation.
5. **Trap instead of corrupt.** Linear-memory effective addresses are bounds-checked before every access.
6. **Defense in depth.** Runtime checks remain even for invariants already proven by validation.
7. **Small vertical slices.** Every phase ends in executable behavior and tests.

See [`docs/architecture.md`](docs/architecture.md) for the current contracts and [`docs/roadmap.md`](docs/roadmap.md) for planned phases.

## Status

Experimental and intentionally incomplete. It is a learning/runtime-engineering project, not a production sandbox.

## License

MIT
