# Phase 5C — Typed Host Function ABI

> Historical slice note: this document records the state of the typed-host ABI at the time this slice landed. The later `phase5c-host-multi-value.md` slice superseded the zero-or-one-result limitation for imported functions by adding `HostRegistry::register_values`; `HostRegistry::register` itself intentionally remains the compatibility zero-or-one-result API. Treat the limitations below as provenance for this slice, not as the repository's current host-result capability.

This slice removes the remaining i32-only restriction from imported host functions while preserving the runtime's existing zero-or-one-result execution model and fail-closed host boundary.

## Scope

Host function imports and registrations may use any MVP numeric value type:

- `i32`
- `i64`
- `f32`
- `f64`

Parameters may mix numeric types in any order. Results remain limited to zero or one value, matching the current function-result bound across the runtime.

## Existing architecture reused

The host callback ABI was already numeric-value generic before this slice:

```text
HostRegistry signature metadata: Vec<ValueType> -> Vec<ValueType>
Host callback arguments:        &[Value]
Host callback result:           Option<Value>
```

Instantiation already compares the registered host signature against the imported WebAssembly function type exactly. Invocation already validates argument count and each runtime `Value` variant against the imported parameter types before entering the callback.

Phase 5C therefore removes only the artificial i32-only registration/validation restrictions instead of introducing a second host ABI.

## Boundary invariants

### Registration

`HostRegistry::register` accepts all four numeric value types but still rejects more than one result.

### Module validation

Function imports may contain all four numeric value types. Import type indices must still exist and imported functions remain limited to zero or one result.

### Instantiation

A registered host function must match the imported parameter and result vectors exactly. A differently typed registration fails before instance construction.

### Callback entry

Before the callback runs, runtime argument values are checked against the imported parameter vector. A wrong runtime variant is rejected before host code observes the call.

### Callback return

After the callback returns:

1. result arity is checked;
2. if a result exists, its `Value` variant is checked against the imported result type;
3. a mismatch becomes `RuntimeError::HostResultTypeMismatch` at the host boundary.

This prevents a malformed or buggy host callback from injecting a differently typed value into the WebAssembly operand stack.

## Floating-point bit behavior

Host callbacks exchange `Value::F32(f32)` and `Value::F64(f64)` directly. No conversion or canonicalization is performed at the host boundary. Tests therefore verify NaN payload preservation through exact `to_bits()` comparisons.

## Metering and capabilities

Typed host values do not alter the existing security/resource model:

- host-call budgets are consumed before callback execution;
- `HostContext` remains capability-scoped;
- memory access remains separately gated by `MEMORY_READ` / `MEMORY_READ_WRITE`;
- host failures continue to map through `HostCallFailed`;
- exact import/registration signature matching remains mandatory.

## Adversarial coverage

The integration suite covers:

- i64 host import round-trip;
- mixed `i32/i64/f32/f64` parameter order and variants;
- f32/f64 NaN payload preservation;
- wrong non-i32 argument rejection before callback invocation;
- wrong host result variant rejection through `HostResultTypeMismatch`;
- multi-result registration remaining fail-closed;
- non-i32 host signature mismatch during instantiation;
- a defined WebAssembly function consuming a typed host result in subsequent numeric execution.

## Non-goals

This slice does not add:

- multi-value host results;
- reference-type host parameters/results;
- SIMD values;
- component-model ABI lowering;
- asynchronous host calls;
- threads or shared-memory semantics.
