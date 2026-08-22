# Phase 5C pinned `if.wast` tranche

## Provenance

This tranche is derived from `WebAssembly/spec` commit `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`, source `test/core/if.wast`. CI uses the committed curated fixture `crates/wasm-runtime/tests/fixtures/phase5c_upstream_if_subset.wast`; it does not fetch a floating upstream branch.

## Selected supported semantics

The fixture keeps only source-faithful cases that fit the runtime's already-supported instruction surface:

- `if` result selection on false and true conditions
- branch from either `then` or `else` carrying a single result
- ordered three-value branch vectors from an `if`
- one- and two-parameter `if` signatures
- distinct then/else arithmetic over block parameters
- branch out of parameterized `if` arms while preserving the declared result

Cases depending on unrelated unsupported operators are intentionally not selected. In particular, this tranche does not widen the product surface merely to accept more of the upstream file.

## Exact accounting

The manifest row requires exactly:

- 1 module
- 12 executed assertions
- 0 filtered directives

The pre-existing `func.wast`, `i32.wast`, `memory.wast`, `block.wast`, and `loop.wast` rows are unchanged. Together the manifest runner executes 48 pinned upstream assertions across six source rows.

A selected assertion becoming filtered, the fixture/source provenance drifting, or the expected counts changing without an explicit manifest update is a test failure.

## Execution path

The existing Phase 5C WAST harness parses the script with the pinned `wast` dev dependency, encodes the core module, parses the resulting bytes with this repository's parser, validates/instantiates with `Instance::new`, and invokes exports through the public runtime API. The `wast` crate is only the text/script front end; it is not a reference runtime oracle.

## Non-goals

This tranche does not claim support for the complete upstream `test/core/if.wast`, the full official WebAssembly spec suite, unsupported directives, or unsupported proposal features. It makes no parser, validator, interpreter, host ABI, dependency, workflow, or CI-policy change.
