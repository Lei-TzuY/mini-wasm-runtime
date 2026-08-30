# Phase 5C — Host Function ABI Boundary

Phase 5C now admits imported Rust host functions across the MVP numeric value types (`i32`, `i64`, `f32`, `f64`) while deliberately retaining the runtime's zero-or-one-result execution boundary.

## Current admitted surface

A function import is executable only when all of the following hold:

- its type index resolves;
- it declares at most one result;
- every parameter and result is one of the supported MVP numeric value types;
- an exact `(module, name)` host registration exists;
- the registered parameter and result vectors exactly match the imported function type;
- runtime argument `Value` variants match the declared parameter types before the callback executes;
- callback result arity matches the declared zero-or-one-result signature;
- a returned callback `Value` matches the declared result `ValueType` before it can enter Wasm execution or escape a direct imported-function export;
- capability and host-call-budget checks remain in force.

The previous i32-only admission gate has been removed at both layers. `wasm-validator` accepts i32/i64/f32/f64 imported-function signatures with at most one result, and `HostRegistry::register` accepts the same surface. Multi-result registrations remain fail closed.

## Runtime safety contract

Mixed-numeric admission does not weaken the host boundary. Instance construction still requires exact registered/imported signature equality. `invoke_host` validates every runtime argument against the declared parameter type before callback execution, checks callback result arity after execution, and then checks the actual returned `Value` variant against the declared result type.

This result-type check is required because registration metadata describes what a callback promises to return while the callback itself returns the generic `Option<Value>` representation and can dynamically violate that promise. Arity checking alone is insufficient.

The admitted path also preserves exact floating-point payload bits. f32/f64 values cross the host boundary as their existing `Value` variants without numeric conversion or NaN canonicalization.

## Adversarial coverage

Phase 5C pins the mixed-numeric boundary with coverage for:

- mixed numeric host signatures and end-to-end execution;
- i64/f32/f64 registration admission while retaining the one-result ceiling;
- exact registered/imported signature matching at instantiation;
- wrong argument variants rejected before callback side effects;
- wrong callback result variants rejected with `RuntimeError::ValueTypeMismatch`;
- wrong-result coverage for i32 as well as non-i32 declared result types;
- exact f32/f64 payload-bit preservation, including nontrivial NaN payloads;
- missing and unexpected callback results rejected by arity checks;
- multi-result registration remaining fail closed.

## Remaining boundary

Multi-value host results remain intentionally unsupported because general multi-value execution is a separate Phase 5C vertical slice. That boundary must not be relaxed independently of the validator/runtime execution model.

## Non-goals

This boundary does not grant implicit filesystem, network, process, environment, WASI, threads/shared-memory, memory64, or reference-type capabilities. Existing `HostCapabilities`, call-depth/fuel/host-call limits, imported memory/table/global identity rules, and CI/MSRV policy remain unchanged.
