# Phase 5C pinned `start.wast` tranche

## Provenance

- Upstream: `WebAssembly/spec@fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- Source: `test/core/start.wast`
- Fixture: `crates/wasm-runtime/tests/fixtures/phase5c_upstream_start_subset.wast`

## Selected source-faithful region

The fixture keeps the contiguous positive MVP region containing two modules. The first names the start function symbolically with `(start $main)`; the second selects the same function by numeric index with `(start 2)`.

Each module initializes memory byte zero to ASCII `A` (65). Its start function calls the same increment helper three times, so the first observable `get` returns 68. Two subsequent bare `(invoke "inc")` directives then advance the same live instance to 69 and 70.

Exact accounting:

- 2 modules
- 6 `assert_return` directives
- 4 stateful bare `invoke` directives
- 0 filters

The earlier invalid-start-signature/index assertions and later spectest-import, trapping-start, and malformed-multiple-start cases remain outside this tranche rather than being filtered into apparent success.

## Runner contract

The manifest now accounts for successful bare invokes separately from assertions. A bare invoke must target the current unnamed supported module, translate only supported numeric arguments, execute successfully, and preserve the instance for later directives. Its return values, if any, are intentionally ignored by script semantics; traps remain failures unless represented by an assertion directive.

## Reference check

The differential workspace includes a Wasmtime case where a start function mutates linear memory three times before exported `run` observes the byte. Both runtimes must return 68 after instantiation.
