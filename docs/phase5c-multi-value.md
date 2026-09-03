# Phase 5C — multi-value results

> Historical slice note: this document records the repository state when defined-function and structured-control multi-value execution first landed. A later Phase-5C slice added the `HostRegistry::register_values` multi-result host callback ABI, so the host-result limitation below is preserved as provenance for this slice rather than a current repository capability claim. For current host behavior, prefer `README.md`, `docs/architecture.md`, `docs/roadmap.md`, and `docs/phase5c-host-multi-value.md`.

This slice extends defined-function and structured-control execution from zero-or-one result to ordered vectors of numeric results.

## Execution model

The operand stack already carries arbitrary value vectors. Multi-value support therefore promotes function and control-frame result metadata from `Option<ValueType>` to `Vec<ValueType>` and preserves values in declared stack order.

Supported in this slice:

- defined function signatures with multiple numeric results;
- exported multi-result defined functions;
- direct calls returning multiple values;
- `call_indirect` returning multiple values after the existing dynamic type check;
- type-index `block` / `if` signatures with multiple results;
- branch label vectors carrying multiple block/function results;
- function return carrying the complete declared result vector.

## Public API compatibility

`Instance::invoke_export_values` is the canonical vector-return API and returns `Vec<Value>` for zero, one, or many results.

The existing `Instance::invoke_export` API remains source-compatible for zero-or-one-result exports. If an export declares multiple results, it rejects the call before execution with `MultiValueResultRequiresValuesApi`; this prevents a compatibility error from executing a side-effecting function and only then discovering that the caller chose the wrong result API.

The CLI uses the vector-return path. Zero and one result retain their prior output shape; multiple values print as an ordered tuple-like list.

## Host ABI boundary

Registered Rust host callbacks remain zero-or-one-result in this slice. Function imports declaring multiple results therefore continue to fail closed during validation with `UnsupportedImportResultArity`.

This is deliberate scope separation: defined WebAssembly multi-value execution no longer depends on redesigning the trusted host callback return type.

## Validation invariants

- function-end stack height equals the complete result-vector arity;
- every result position must match the declared type in order;
- direct and indirect calls push every result in declaration order;
- loop labels continue to carry block parameters, while block/if/function labels carry complete result vectors;
- an `if` with any results still requires an `else`;
- type-index block signatures may carry multiple results, while inline block types remain zero-or-one-result by encoding.

## Non-goals

This slice does not add:

- multi-result Rust host callback registration;
- reference types beyond the existing funcref table subset;
- exception handling;
- tail calls;
- WASI;
- threads/shared memory.
