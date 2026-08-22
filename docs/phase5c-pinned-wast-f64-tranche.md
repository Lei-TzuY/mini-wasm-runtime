# Phase 5C pinned `f64.wast` tranche

This tranche extends the repository-committed upstream WAST manifest with a curated subset of `WebAssembly/spec` `test/core/f64.wast` from commit `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`.

The fixture `crates/wasm-runtime/tests/fixtures/phase5c_upstream_f64_subset.wast` is intentionally limited to f64 semantics already implemented by the runtime. It does not claim support for the complete upstream file.

## Selected semantics

The tranche executes 14 assertions covering:

- signed-zero preservation in addition;
- addition at the smallest positive f64 subnormal boundary;
- finite subtraction, multiplication, and division;
- square root;
- signed-zero selection for `f64.min` and `f64.max`;
- `ceil`, `floor`, and `trunc` around negative fractional values;
- `nearest` ties-to-even for 2.5 and 3.5;
- `nearest(-0.5)` preserving negative zero.

Expected ordinary f64 values are compared through the WAST runner by exact raw bits. This makes signed-zero and rounding regressions observable instead of allowing numeric equality to hide them.

## Manifest and failure policy

The manifest row requires exactly one module, 14 executed assertions, and zero filtered directives. Existing provenance guards still require the exact pinned commit, a unique `test/core/*.wast` source, a unique registered fixture, and matching commit/source markers inside the fixture.

Any selected assertion becoming filtered, any accounting drift, or any exact result-bit mismatch is a regression. The tranche does not widen the WAST filter, NaN matching, parser, validator, interpreter, host ABI, or CI policy.

## Execution path

The fixture follows the existing Phase 5C ingestion path:

1. parse WAST with the pinned test-only `wast` dependency;
2. encode the core module;
3. parse the resulting Wasm bytes with this repository's parser;
4. validate and instantiate with this repository's runtime path;
5. invoke exports and compare results using the existing exact-value WAST assertions.

CI remains network-independent because the curated fixture and provenance manifest are committed in the repository.

## Non-goals

This tranche does not claim complete `test/core/f64.wast` coverage, complete floating-point conformance, or complete WebAssembly spec-suite support. Broader pinned manifest expansion remains separate, reviewable work.
