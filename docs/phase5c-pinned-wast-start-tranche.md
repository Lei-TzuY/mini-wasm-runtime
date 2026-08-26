# Phase 5C pinned `start.wast` tranche

## Provenance

- Upstream: `WebAssembly/spec@fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- Source: `test/core/start.wast`
- Fixture: `crates/wasm-runtime/tests/fixtures/phase5c_upstream_start_subset.wast`

## Selected source-faithful directives

The fixture now covers three distinct start-function phases without filtering unsupported directives into apparent success.

Validation-time coverage keeps the first three upstream `assert_invalid` directives:

- an out-of-bounds start index must fail as `StartFunctionOutOfBounds`
- a result-producing start function must fail as `InvalidStartSignature`
- a parameter-taking start function must fail as `InvalidStartSignature`

The positive stateful region contains two live modules. The first names the start function symbolically with `(start $main)`; the second selects the same function by numeric index with `(start 2)`. Each initializes memory byte zero to ASCII `A` (65), runs three increments during instantiation, then preserves the same live instance across bare invokes so observations progress 68 -> 69 -> 70.

Finally, the upstream trapping-start directive validates successfully and must fail specifically during instantiation/start execution with the `unreachable` runtime trap.

Exact accounting:

- 2 live module directives
- 10 executed assertions: 3 invalid + 6 return + 1 trap
- 4 stateful bare `invoke` directives
- 0 filters

The spectest-import modules and malformed multiple-start directive remain outside this tranche because they require separate host-import/script-malformed capabilities; they are not counted as filters.

## Runner contract

`assert_invalid` is phase-sensitive: selected directives must encode and parse structurally, then fail static validation with the expected `ValidationError` class. A later linking or runtime failure cannot satisfy an invalid assertion.

`assert_trap` now supports both exported invocations and inline core modules. Inline modules must parse and validate first; only instantiation/start execution may produce the expected `RuntimeError`. The current unnamed live module is not replaced by an inline trapping assertion.

Bare invokes remain separately accounted stateful actions against the current live module.

## Reference checks

The differential workspace retains the successful-start memory-side-effect case and adds an instantiation-time trapping-start case. Both mini-wasm-runtime and Wasmtime must normalize the latter to the unreachable trap class.
