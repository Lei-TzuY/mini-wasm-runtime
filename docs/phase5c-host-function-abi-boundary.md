# Phase 5C — Host Function ABI Boundary

Phase 5C currently keeps imported Rust host functions on the original i32-only, zero-or-one-result ABI. This is an explicit admission boundary, not an accidental limitation to be removed one layer at a time.

## Current admitted surface

A function import is executable only when all of the following hold:

- its type index resolves;
- it declares at most one result;
- every parameter and result is `i32`;
- an exact `(module, name)` host registration exists;
- the registered parameter and result vectors exactly match the imported function type;
- runtime argument `Value` variants match the declared parameter types before the callback executes;
- callback result arity matches the declared zero-or-one-result signature;
- capability and host-call-budget checks remain in force.

The i32-only rule is deliberately enforced twice. `wasm-validator` rejects non-i32 imported function parameters/results, and `HostRegistry::register` rejects the same signatures before they can become runtime bindings. `crates/wasm-runtime/tests/phase5c_host_function_abi_boundary.rs` pins both layers for i64, f32, and f64.

## Required mixed-numeric vertical slice

The future non-i32 host-function slice must not merely remove those two admission gates. It is complete only when the entire boundary moves together:

1. validator admission accepts i32/i64/f32/f64 parameters and zero-or-one numeric result;
2. `HostRegistry::register` accepts the same numeric signature surface while retaining the one-result ceiling;
3. instance construction still requires exact registered/imported signature equality;
4. callback arguments remain variant-checked before callback execution;
5. callback result arity remains checked after callback execution;
6. a returned `Value` is checked against the declared result `ValueType` before it can enter Wasm execution or escape a direct imported-function export;
7. a wrong callback result variant fails with a dedicated typed host-boundary error rather than being accepted because its arity is one;
8. f32/f64 values preserve their exact payload bits across the host boundary, including NaN payloads;
9. multi-result host callbacks remain out of scope for this zero-or-one-result slice.

The result-type check is a defense-in-depth requirement. Registration metadata describes what a callback promises to return, but the callback itself returns the generic `Option<Value>` type and can dynamically violate that promise. Arity checking alone is therefore insufficient once the boundary is generalized—and is not a substitute for value-type checking even for the current generic callback representation.

## Required adversarial coverage

A complete mixed-numeric implementation should cover at least:

- i64 parameter/result round-trip;
- mixed `[i32, i64, f32, f64]` parameter order and exact variants;
- exact f32/f64 NaN payload preservation;
- wrong argument variant rejected before callback side effects;
- wrong callback result variant rejected at the host boundary;
- exact non-i32 registration/import signature mismatch at instantiation;
- a defined Wasm caller consuming a typed host result in subsequent typed execution;
- multi-result registration remaining fail closed.

These requirements intentionally match the runtime's existing zero-or-one-result execution model. Multi-value execution is a separate vertical slice and must not be smuggled into this change merely to broaden the host ABI.

## Non-goals

This boundary does not grant implicit filesystem, network, process, environment, WASI, threads/shared-memory, memory64, or reference-type capabilities. Existing `HostCapabilities`, call-depth/fuel/host-call limits, imported memory/table/global identity rules, and CI/MSRV policy remain unchanged.
