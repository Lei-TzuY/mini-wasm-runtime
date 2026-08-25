# Phase 5C — Host multi-value callback ABI

This slice closes the remaining multi-value gap between defined WebAssembly functions and imported Rust host functions.

## Public registration APIs

`HostRegistry::register` remains source-compatible with the original callback shape:

```text
&[Value] -> Result<Option<Value>, HostError>
```

It continues to accept only zero-or-one declared result so existing embeddings do not need to change.

`HostRegistry::register_values` is the vector-return API:

```text
&[Value] -> Result<Vec<Value>, HostError>
```

Its declared result vector may contain zero, one, or many i32/i64/f32/f64 values. Both APIs share the same registry namespace, exact signature matching, capability set, and host-call metering.

## Validation and execution

Function imports are no longer rejected merely because their function type has multiple results. Their type index must still resolve normally.

At instantiation, the registered parameter and result vectors must exactly equal the imported WebAssembly function type. At callback return, the runtime checks:

1. returned vector length equals the declared result arity;
2. every returned `Value` variant matches the corresponding declared result type in order;
3. only then may the vector enter the WebAssembly operand stack.

`invoke_export_values` therefore works uniformly for imported, defined, and forwarding functions with zero/one/many results.

The compatibility `invoke_export` API still rejects any multi-result export before execution. This includes an export that forwards to a host import, so selecting the wrong embedding API cannot trigger host-side effects before returning `MultiValueResultRequiresValuesApi`.

## Security/resource invariants

Multi-result callbacks do not bypass existing host controls:

- host-call budget is consumed before callback entry;
- `HostContext` remains capability-scoped;
- host callback failures remain `HostCallFailed`;
- malformed result arity becomes `HostResultArityMismatch`;
- malformed result types become `HostResultTypeMismatch` before values reach guest execution;
- exact import/registration signature matching still occurs before instance construction.

## Verification

Coverage includes ordered mixed numeric results forwarded through defined Wasm, legacy registration compatibility, pre-execution legacy invocation rejection, wrong arity, wrong result type, signature mismatch, and a Wasmtime differential case using the same multi-result imported function bytes.

## Non-goals

This does not add reference-type host values, asynchronous callbacks, component-model ABI lowering, WASI, threads/shared-memory proposal semantics, or host re-entrancy into the same `Instance`.
