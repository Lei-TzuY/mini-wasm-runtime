# Phase 5C pinned `start.wast` tranche

## Provenance

- Upstream: `WebAssembly/spec@fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- Source: `test/core/start.wast`
- Fixture: `crates/wasm-runtime/tests/fixtures/phase5c_upstream_start_subset.wast`

## Selected source-faithful directives

The fixture now covers the complete pinned `start.wast` source: validation-time rejection, successful start execution (including standard `spectest` function imports), stateful post-instantiation execution, instantiation-time trapping, and the final quoted malformed duplicate-start directive, with zero filters.

Validation-time coverage keeps the first three upstream `assert_invalid` directives:

- an out-of-bounds start index must fail as `StartFunctionOutOfBounds`
- a result-producing start function must fail as `InvalidStartSignature`
- a parameter-taking start function must fail as `InvalidStartSignature`

The positive stateful region contains two live modules. The first names the start function symbolically with `(start $main)`; the second selects the same function by numeric index with `(start 2)`. Each initializes memory byte zero to ASCII `A` (65), runs three increments during instantiation, then preserves the same live instance across bare invokes so observations progress 68 -> 69 -> 70.

The next three upstream positive modules exercise imported start calls through the standard test harness. Two import `spectest.print_i32` with signature `[i32] -> []` and call it from symbolic/numeric start functions with values 1 and 2; the third imports zero-argument `spectest.print` and uses that import itself as the start function. The harness binds both functions as typed no-op callbacks, so successful module instantiation proves import resolution and start-time host invocation are admitted without inventing observable output semantics.

The upstream trapping-start directive validates successfully and must fail specifically during instantiation/start execution with the `unreachable` runtime trap. The final `assert_malformed` keeps the source-faithful quoted module with two start declarations. Its upstream canonical wording `multiple start sections` maps to `MalformedKind::MultipleStartSections`; the pinned `wast = 217.0.0` text parser must then fail during quoted-text WAT/encode with the exact internal message `multiple start sections found`. No binary module is produced for mini-wasm-runtime to parse.

Exact accounting:

- 5 live module directives: 2 stateful memory modules + 3 `spectest` imported-start modules
- 11 executed assertions: 3 invalid + 6 return + 1 trap + 1 malformed
- 4 stateful bare `invoke` directives
- 0 filters

This accounts for every directive in the pinned upstream `test/core/start.wast` source.

## Runner contract

`assert_invalid` is phase-sensitive: selected directives must encode and parse structurally, then fail static validation with the expected `ValidationError` class. A later linking or runtime failure cannot satisfy an invalid assertion.

`assert_trap` supports both exported invocations and inline core modules. Inline modules must parse and validate first; only instantiation/start execution may produce the expected `RuntimeError`. The current unnamed live module is not replaced by an inline trapping assertion.

`assert_malformed` is also phase-sensitive: this slice accepts only quoted core modules, translates the pinned upstream wording into a typed malformed class, requires WAT/text encoding to fail before any mini binary parse or validation can occur, and then matches that class to the exact pinned `wast` parser message. Bare invokes remain separately accounted stateful actions against the current live module.

## Reference checks

The differential workspace retains the successful-start memory-side-effect and trapping-start cases. It also instantiates a module whose start function calls imported `print_i32` and `print` callbacks, recording a shared trace on both mini-wasm-runtime and Wasmtime. Both engines must execute the callbacks during instantiation in the same order and with the same i32 argument. Separately, the binary negative-conformance corpus constructs two physical start sections and requires mini-wasm-runtime's binary parser to return `ParseError::DuplicateSection(8)`, keeping the text-malformed and binary-malformed layers independently typed.
