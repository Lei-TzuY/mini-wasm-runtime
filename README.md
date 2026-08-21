# mini-wasm-runtime

A small WebAssembly runtime built from first principles in Rust.

The project intentionally does **not** embed Wasmtime/Wasmer or another WebAssembly engine. The parser, validator, interpreter, numeric value model, linear memory, tables, globals, and host boundary live in this repository so the interesting invariants stay visible and testable.

## Current milestone: Phase 5B

Phase 5B replaces the old i32-specialized validation/execution model with a typed MVP numeric core while preserving the Phase-5A state/table and Phase-4 host boundaries:

- WebAssembly binary header plus u32/i32/i64 LEB128 decoding
- type, function-import, function, table, memory, global, export, start, element, code, and active data sections
- `i32`, `i64`, `f32`, and `f64` defined-function parameters, locals, zero-or-one results, globals, and block results
- a single typed operand-stack validator; the legacy arity-only validator has been removed
- typed direct calls and `call_indirect`, including non-i32 defined-function signatures
- mutable/immutable numeric globals with `global.get` / `global.set`
- `funcref` table plus active table-0 element segments and precise indirect-call traps
- structured `block`, `loop`, `if`, `else`, `br`, and `br_if` with exact result-type convergence
- numeric constants, integer/floating comparisons, i32/i64 arithmetic, f32/f64 arithmetic, and selected non-trapping conversions
- one 32-bit linear memory with 64 KiB pages and the existing checked i32 load/store family
- typed `HostRegistry` and capability-scoped `HostContext`; function imports remain intentionally i32-only in this slice
- configurable call-depth, memory-page, instruction-fuel, and host-call limits
- CLI inspect/run commands and multi-platform CI plus Rust 1.81 MSRV validation

### Executable instruction subset

Control/state: `block`, `loop`, `if`, `else`, `br`, `br_if`, `return`, `end`, `call`, `call_indirect`, `local.get`, `local.set`, `local.tee`, `global.get`, `global.set`.

Numeric constants/comparisons: `i32.const`, `i64.const`, `f32.const`, `f64.const`; i32/i64 `eqz`, `eq`, `ne`, signed/unsigned `lt`, `gt`, `le`, `ge`; f32/f64 `eq`, `ne`, `lt`, `gt`, `le`, `ge`.

Arithmetic/conversions: i32/i64 `add`, `sub`, `mul`; f32/f64 `add`, `sub`, `mul`, `div`; `i32.wrap_i64`, `i64.extend_i32_s`, `i64.extend_i32_u`, `f32.demote_f64`, `f64.promote_f32`.

Memory: `i32.load`, `i32.load8_s`, `i32.load8_u`, `i32.load16_s`, `i32.load16_u`, `i32.store`, `i32.store8`, `i32.store16`, `memory.size`, `memory.grow`.

Function and block results remain limited to zero or one numeric value. The runtime supports at most one defined table and one linear memory. Imports remain function-only and the host ABI remains i32-only. Trapping float-to-integer conversions, reinterpret instructions, broader numeric operators, broader segment/import forms, multi-value, and complete spec-test conformance remain later work.

## Quick start

```bash
cargo test --workspace
cargo run -p wasm-cli -- inspect path/to/module.wasm
cargo run -p wasm-cli -- run path/to/module.wasm add 20 22
cargo run -p wasm-cli -- run path/to/module.wasm some_i64_export i64:42
cargo run -p wasm-cli -- run path/to/module.wasm some_float_export f32:1.5 f64:2.5
```

CLI values default to `i32`; use explicit `i64:`, `f32:`, or `f64:` prefixes for the other numeric types. `mini-wasm inspect` reports imports, function index space, tables, memories, typed globals, exports, start function, and element/data segment counts.

The standalone CLI intentionally installs no implicit host functions or capabilities. Executing a module with imports therefore requires an embedding application to construct a `HostRegistry` and instantiate with `Instance::with_hosts` or `Instance::with_config`.

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

Callbacks receive a `HostContext`, not the `Instance`. Memory access is denied unless the registration explicitly grants `MEMORY_READ` or `MEMORY_READ_WRITE`. Phase 5B deliberately keeps imported function signatures i32-only even though defined WebAssembly code now supports all four MVP numeric types.

## Workspace

```text
crates/
  wasm-parser/     binary format + typed numeric constants + module sections
  wasm-validator/  typed operand/control stacks + index/state/table/memory invariants
  wasm-runtime/    typed interpreter + globals + table + linear memory + host boundary
  wasm-cli/        inspect/run command-line frontend
docs/
  architecture.md
  phase5b-numeric-model.md
  roadmap.md
```

## Design principles

1. **Fail closed.** Unsupported binary features, import kinds, segment modes, and opcodes are errors.
2. **No hidden engine dependency.** Runtime behavior is implemented here.
3. **Parser != validator != executor.** Each layer has a narrow contract.
4. **Validate types, not just arity.** Every reachable operand slot carries a `ValueType`; locals, globals, calls, labels, and control results must match exactly.
5. **Trap precisely.** Indirect calls distinguish out-of-bounds, null entries, and dynamic type mismatches; memory ranges are checked before access.
6. **Least capability at the host boundary.** Host callbacks receive only explicitly granted facilities.
7. **Meter untrusted work.** Embedders may cap call depth, memory pages, instruction fuel, and host calls.
8. **Defense in depth.** Runtime type, stack, control, memory, table, and host checks remain even after static validation.
9. **Small vertical slices.** Every phase ends in executable behavior and tests.

See [`docs/architecture.md`](docs/architecture.md), [`docs/phase5b-numeric-model.md`](docs/phase5b-numeric-model.md), and [`docs/roadmap.md`](docs/roadmap.md).

## Status

Experimental and intentionally incomplete. It is a learning/runtime-engineering project, not a production sandbox.

## License

MIT
