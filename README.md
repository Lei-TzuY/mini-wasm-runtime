# mini-wasm-runtime

A small WebAssembly runtime built from first principles in Rust.

The project intentionally does **not** embed Wasmtime, Wasmer, or another WebAssembly engine. The parser, validator, interpreter, numeric core, linear memory, tables, globals, and host boundary are implemented in this repository so their invariants remain visible and testable.

## Current status

The current baseline combines the completed Phase 1–5 implementation with a substantial Phase 6 engineering-hardening layer. It is an experimental interpreter/runtime-engineering project, not a production sandbox.

Major implemented surfaces include:

- WebAssembly binary parsing for the supported MVP-oriented module surface
- independent function/table/memory/global index spaces
- typed operand/control-stack validation with unreachable-stack polymorphism
- `block`, `loop`, `if`, `br`, `br_if`, `br_table`, `return`, `nop`, `drop`, `select`, direct calls, and `call_indirect`
- ordered multi-value results for defined Wasm functions and structured control
- i32/i64/f32/f64 values, arithmetic, comparisons, integer bit operations, conversions, reinterpretation, and saturating conversions
- typed i32/i64/f32/f64 memory loads/stores
- one 32-bit linear memory with bounds checks, data segments, `memory.size`, and `memory.grow`
- defined and imported numeric globals
- defined and imported `funcref` tables with instance-bound function references
- defined and imported memory with shared host-visible backing
- active/passive/declarative legacy segment forms supported by the current parser/runtime boundary
- typed host functions with explicit capability-scoped memory access
- configurable call-depth, memory-page, instruction-fuel, and host-call limits
- CLI inspect/run support with typed arguments and ordered multi-value output

Host callbacks support i32/i64/f32/f64 parameters and results. `HostRegistry::register` remains the source-compatible zero-or-one-result API, while `HostRegistry::register_values` returns an ordered `Vec<Value>` for zero, one, or many host results.

## Conformance and hardening

The repository includes more than feature tests. The current baseline also contains:

- cross-layer negative-conformance tests for parser, validator, instantiation, and runtime failures
- malformed-binary parser corpus and untrusted-count allocation hardening
- deterministic property/metamorphic tests, including generated expression/control, multi-value, table-dispatch, imported-state, and stateful-memory domains
- deterministic parser/validator mutation robustness tests
- cargo-fuzz parser and parse-to-validation targets with scheduled coverage-guided, sanitizer-backed campaigns, corpus minimization, and source-coverage report automation
- isolated Wasmtime differential execution with deterministic generators, normalized trap classes, mismatch shrinking, replay fixtures, and imported-state/host-boundary coverage
- deterministic interpreter benchmarks plus controlled-host median/MAD baseline comparison tooling
- executable runtime security invariants plus a documented threat model
- pinned upstream WebAssembly spec provenance
- WAST ingestion infrastructure with exact executed/filtered accounting

The committed WAST manifest is pinned to `WebAssembly/spec` commit `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`. At this consolidation point it covers 33 unique upstream sources, 1074 selected assertions, and 4 stateful bare invokes with zero filters.

## Known boundaries

This is intentionally incomplete. Important remaining work includes:

- broader official WAST/spec-suite coverage
- broader malformed/adversarial runtime corpora
- recording and periodically checking a reviewed performance baseline on a pinned controlled host
- WASI
- threads/shared-memory proposal semantics
- SIMD, memory64, multi-memory/multi-table
- JIT compilation

Unsupported binary features, instructions, imports, and execution forms are expected to fail closed rather than being silently approximated.

## Quick start

```bash
cargo test --workspace
cargo run -p wasm-cli -- inspect path/to/module.wasm
cargo run -p wasm-cli -- run path/to/module.wasm add 20 22
cargo run -p wasm-cli -- run path/to/module.wasm some_i64_export i64:42
cargo run -p wasm-cli -- run path/to/module.wasm some_float_export f32:1.5 f64:2.5
```

CLI values default to `i32`; use `i64:`, `f32:`, or `f64:` prefixes for the other numeric types.

The standalone CLI installs no implicit host imports or capabilities. Modules that import functions, globals, tables, or memory must be instantiated by an embedding application with an explicit `HostRegistry`.

## Embedding

The host boundary is typed and explicit:

```rust
use wasm_parser::ValueType;
use wasm_runtime::{
    GlobalHandle, HostCapabilities, HostRegistry, Instance, MemoryHandle, TableHandle, Value,
};

let mut hosts = HostRegistry::new();

hosts.register(
    "env",
    "double_i64",
    vec![ValueType::I64],
    vec![ValueType::I64],
    HostCapabilities::NONE,
    |_ctx, args| Ok(Some(Value::I64(args[0].as_i64().wrapping_mul(2)))),
)?;

hosts.register_values(
    "env",
    "split",
    vec![ValueType::I32],
    vec![ValueType::I32, ValueType::I64],
    HostCapabilities::NONE,
    |_ctx, args| Ok(vec![Value::I32(args[0].as_i32()), Value::I64(args[0].as_i32() as i64)]),
)?;

let counter = GlobalHandle::mutable(Value::I32(0));
hosts.register_global("env", "counter", counter.clone())?;

let table = TableHandle::new(4, Some(16))?;
hosts.register_table("env", "dispatch", table.clone())?;

let memory = MemoryHandle::new(1, Some(8))?;
hosts.register_memory("env", "memory", memory.clone())?;

let mut instance = Instance::with_hosts(module, hosts)?;
```

Callbacks receive a `HostContext`, not the entire `Instance`. Memory helpers are denied unless the registration explicitly grants `MEMORY_READ` or `MEMORY_READ_WRITE`.

Imported mutable globals, tables, and memory use shared backing rather than copy semantics. Host-side changes are visible to Wasm and Wasm-side changes are visible through the retained handles. Active segment initialization is preflighted before host-shared targets are mutated so failed instantiation does not leave partial segment writes.

For exported functions:

- `Instance::invoke_export(...)` is the compatibility API for zero-or-one result
- `Instance::invoke_export_values(...)` returns an ordered `Vec<Value>` for zero/one/many results

Calling a multi-result export through the legacy zero-or-one API fails before execution.

## Workspace

```text
crates/
  wasm-parser/     binary format, sections, imports, typed constants
  wasm-validator/  typed operand/control stacks and module invariants
  wasm-runtime/    interpreter, memory/tables/globals, host boundary
  wasm-cli/        inspect/run command-line frontend
docs/
  architecture.md
  roadmap.md
  security-threat-model.md
  phase5c-*.md
  phase6-*.md
```

## Design principles

1. **Fail closed.** Unsupported syntax or semantics are errors, not implicit no-ops.
2. **No hidden engine dependency.** Runtime behavior is implemented here.
3. **Parser != validator != executor.** Each layer owns a narrow contract.
4. **Respect independent index spaces.** Function, table, memory, and global ordinals never share arithmetic.
5. **Validate types, not just arity.** Reachable operand slots carry exact `ValueType`s.
6. **Model unreachable code correctly.** Control-stack polymorphism must not weaken opcode validation.
7. **Trap precisely.** Memory, numeric, indirect-call, and host-boundary failures stay distinguishable.
8. **Least capability at the host boundary.** Host callbacks receive only explicitly granted runtime facilities.
9. **Do not fake aliasing.** Imported mutable state uses shared backing.
10. **Preflight host-visible initialization.** Failed instantiation must not leak partial segment mutations.
11. **Meter untrusted work.** Embedders can cap call depth, memory, fuel, and host calls.
12. **Defense in depth.** Runtime checks remain even after static validation.
13. **Deterministic tests first.** CI, property corpora, mutation tests, and pinned conformance inputs must be reproducible.

See [`docs/architecture.md`](docs/architecture.md), [`docs/roadmap.md`](docs/roadmap.md), and [`docs/security-threat-model.md`](docs/security-threat-model.md).

## CI

Every pull request is checked on:

- Rust stable / Ubuntu
- Rust stable / Windows
- Rust stable / macOS
- Rust 1.81.0 / Ubuntu (declared MSRV)

Each matrix job runs formatting, Clippy with warnings denied, the full workspace test suite, and documentation builds.

## License

MIT
