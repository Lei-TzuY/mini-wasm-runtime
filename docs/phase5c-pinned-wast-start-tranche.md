# Phase 5C pinned `start.wast` tranche

## Provenance

- Upstream: `WebAssembly/spec@fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- Source: `test/core/start.wast`
- Fixture: `crates/wasm-runtime/tests/fixtures/phase5c_upstream_start_subset.wast`

## Selected source-faithful directives

The fixture covers validation-time rejection, successful start execution (including standard `spectest` function imports), stateful post-instantiation execution, and instantiation-time trapping without filtering unsupported directives into apparent success.

Validation-time coverage keeps the first three upstream `assert_invalid` directives:

- an out-of-bounds start index must fail as `StartFunctionOutOfBounds`
- a result-producing start function must fail as `InvalidStartSignature`
- a parameter-taking start function must fail as `InvalidStartSignature`

The positive stateful region contains two live modules. The first names the start function symbolically with `(start $main)`; the second selects the same function by numeric index with `(start 2)`. Each initializes memory byte zero to ASCII `A` (65), runs three increments during instantiation, then preserves the same live instance across bare invokes so observations progress 68 -> 69 -> 70.

The next three upstream positive modules exercise imported start calls through the standard test harness. Two import `spectest.print_i32` with signature `[i32] -> []` and call it from symbolic/numeric start functions with values 1 and 2; the third imports zero-argument `spectest.print` and uses that import itself as the start function. The harness binds both functions as typed no-op callbacks, so successful module instantiation proves import resolution and start-time host invocation are admitted without inventing observable output semantics.

Finally, the upstream trapping-start directive validates successfully and must fail specifically during instantiation/start execution with the `unreachable` runtime trap.

Exact accounting:

- 5 live module directives: 2 stateful memory modules + 3 `spectest` imported-start modules
- 10 executed assertions: 3 invalid + 6 return + 1 trap
- 4 stateful bare `invoke` directives
- 0 filters

The final malformed multiple-start quoted-module directive remains outside this tranche because it requires script-level `assert_malformed`/quoted-module handling; it is not counted as a filter.

## Runner contract

`assert_invalid` is phase-sensitive: selected directives must encode and parse structurally, then fail static validation with the expected `ValidationError` class. A later linking or runtime failure cannot satisfy an invalid assertion.

`assert_trap` now supports both exported invocations and inline core modules. Inline modules must parse and validate first; only instantiation/start execution may produce the expected `RuntimeError`. The current unnamed live module is not replaced by an inline trapping assertion.

Bare invokes remain separately accounted stateful actions against the current live module.

## Reference checks

The differential workspace retains the successful-start memory-side-effect and trapping-start cases. It also instantiates a module whose start function calls imported `print_i32` and `print` callbacks, recording a shared trace on both mini-wasm-runtime and Wasmtime. Both engines must execute the callbacks during instantiation in the same order and with the same i32 argument.
