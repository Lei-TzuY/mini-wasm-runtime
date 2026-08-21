# mini-wasm-runtime

A small WebAssembly runtime built from first principles in Rust.

The project intentionally does **not** embed Wasmtime/Wasmer or another WebAssembly engine. The parser, validator, interpreter, numeric value model, linear memory, tables, globals, and host boundary live in this repository so the interesting invariants stay visible and testable.

## Current milestone: Phase 5C

Phase 5C is broadening module forms and conformance without weakening the fail-closed boundary established in earlier phases:

- WebAssembly binary header plus u32/i32/i64 and signed-33 blocktype LEB128 decoding
- type, import, function, table, memory, global, export, start, element, code, and active data sections
- explicit function/table/memory/global import descriptors with independent WebAssembly index spaces
- `i32`, `i64`, `f32`, and `f64` defined-function parameters, locals, zero-or-one results, globals, and block results
- a single typed operand-stack validator; the legacy arity-only validator has been removed
- structured `block`, `loop`, and `if` signatures from immediate value types or type indices, including block parameters and correctly typed loop labels
- typed direct calls and `call_indirect`, including non-i32 defined-function signatures
- mutable/immutable defined numeric globals with `global.get` / `global.set`
- executable immutable and mutable numeric global imports resolved through shared `GlobalHandle` backing
- `funcref` table plus active table-0 element segments and precise indirect-call traps
- numeric constants, integer/floating comparisons, i32/i64 arithmetic, f32/f64 arithmetic, and selected non-trapping conversions
- one 32-bit linear memory with 64 KiB pages and the checked i32 load/store family
- typed host functions with capability-scoped `HostContext`; imported host functions remain intentionally i32-only
- configurable call-depth, memory-page, instruction-fuel, and host-call limits
- CLI inspect/run commands and multi-platform CI plus Rust 1.81 MSRV validation

### Executable instruction subset

Control/state: `block`, `loop`, `if`, `else`, `br`, `br_if`, `return`, `end`, `call`, `call_indirect`, `local.get`, `local.set`, `local.tee`, `global.get`, `global.set`.

Numeric constants/comparisons: `i32.const`, `i64.const`, `f32.const`, `f64.const`; i32/i64 `eqz`, `eq`, `ne`, signed/unsigned `lt`, `gt`, `le`, `ge`; f32/f64 `eq`, `ne`, `lt`, `gt`, `le`, `ge`.

Arithmetic/conversions: i32/i64 `add`, `sub`, `mul`; f32/f64 `add`, `sub`, `mul`, `div`; `i32.wrap_i64`, `i64.extend_i32_s`, `i64.extend_i32_u`, `f32.demote_f64`, `f64.promote_f32`.

Memory: `i32.load`, `i32.load8_s`, `i32.load8_u`, `i32.load16_s`, `i32.load16_u`, `i32.store`, `i32.store8`, `i32.store16`, `memory.size`, `memory.grow`.

Function and block results remain limited to zero or one numeric value. The runtime supports at most one table and one linear memory across imported and defined objects. The parser and validator understand function/table/memory/global import descriptors. Execution resolves function imports plus immutable and mutable numeric global imports. Table and memory imports remain fail-closed until equivalent shared backing/aliasing semantics are implemented. Host function signatures remain i32-only.

Trapping float-to-integer conversions, reinterpret instructions, broader numeric operators, broader segment modes, multi-value execution, and complete spec-test conformance remain later work.

## Quick start

```bash
cargo test --workspace
cargo run -p wasm-cli -- inspect path/to/module.wasm
cargo run -p wasm-cli -- run path/to/module.wasm add 20 22
cargo run -p wasm-cli -- run path/to/module.wasm some_i64_export i64:42
cargo run -p wasm-cli -- run path/to/module.wasm some_float_export f32:1.5 f64:2.5
```

CLI values default to `i32`; use explicit `i64:`, `f32:`, or `f64:` prefixes for the other numeric types. `mini-wasm inspect` reports import kinds, independent index-space counts, defined objects, typed globals, exports, start function, and element/data segment counts.

The standalone CLI intentionally installs no implicit host functions, globals, or capabilities. Executing a module with imports therefore requires an embedding application to construct a `HostRegistry` and instantiate with `Instance::with_hosts` or `Instance::with_config`.

## Embedding host bindings

```rust
use wasm_parser::ValueType;
use wasm_runtime::{GlobalHandle, HostCapabilities, HostRegistry, Instance, Value};

let mut hosts = HostRegistry::new();
hosts.register(
    "env",
    "double",
    vec![ValueType::I32],
    vec![ValueType::I32],
    HostCapabilities::NONE,
    |_ctx, args| Ok(Some(Value::I32(args[0].as_i32().wrapping_mul(2)))),
)?;

hosts.register_immutable_global("env", "build_id", Value::I64(42))?;

let counter = GlobalHandle::mutable(Value::I32(0));
hosts.register_global("env", "counter", counter.clone())?;

let mut instance = Instance::with_hosts(module, hosts)?;
```

Callbacks receive a `HostContext`, not the `Instance`. Memory access is denied unless the function registration explicitly grants `MEMORY_READ` or `MEMORY_READ_WRITE`. Global imports require exact numeric type and mutability matching. A mutable `GlobalHandle` is shared between the embedding application and the instance: host writes are immediately visible to `global.get`, and WebAssembly `global.set` updates the same handle. This runtime is currently single-threaded; `GlobalHandle` deliberately uses single-threaded shared ownership rather than pretending to provide threads/shared-memory semantics.

## Workspace

```text
crates/
  wasm-parser/     binary format + import descriptors + typed constants + module sections
  wasm-validator/  typed operand/control stacks + independent index-space invariants
  wasm-runtime/    typed interpreter + shared globals + table + linear memory + host boundary
  wasm-cli/        inspect/run command-line frontend
docs/
  architecture.md
  phase5b-numeric-model.md
  phase5c-module-forms.md
  roadmap.md
```

## Design principles

1. **Fail closed.** Unsupported binary features, runtime import kinds, segment modes, and opcodes are errors.
2. **No hidden engine dependency.** Runtime behavior is implemented here.
3. **Parser != validator != executor.** Each layer has a narrow contract.
4. **Respect independent index spaces.** Function, table, memory, and global imports never share ordinal arithmetic.
5. **Validate types, not just arity.** Every reachable operand slot carries a `ValueType`; locals, globals, calls, labels, and control results must match exactly.
6. **Trap precisely.** Indirect calls distinguish out-of-bounds, null entries, and dynamic type mismatches; memory ranges are checked before access.
7. **Least capability at the host boundary.** Host callbacks receive only explicitly granted facilities.
8. **Do not fake aliasing.** Mutable globals use true shared backing; table/memory imports remain rejected until they can preserve identity and mutation visibility too.
9. **Meter untrusted work.** Embedders may cap call depth, memory pages, instruction fuel, and host calls.
10. **Defense in depth.** Runtime type, stack, control, memory, table, global, and host checks remain even after static validation.
11. **Small vertical slices.** Every phase increment ends in executable behavior and adversarial tests.

See [`docs/architecture.md`](docs/architecture.md), [`docs/phase5b-numeric-model.md`](docs/phase5b-numeric-model.md), [`docs/phase5c-module-forms.md`](docs/phase5c-module-forms.md), and [`docs/roadmap.md`](docs/roadmap.md).

## Status

Experimental and intentionally incomplete. It is a learning/runtime-engineering project, not a production sandbox.

## License

MIT
