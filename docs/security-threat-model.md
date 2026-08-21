# Security threat model

This document describes the security boundary of the current interpreter and the invariants that the normal test suite is expected to preserve. It is intentionally narrower than a claim of sandbox equivalence with a production WebAssembly engine.

## Trust model

### Untrusted

The runtime must treat these as untrusted:

- raw WebAssembly module bytes before parsing;
- module-declared lengths, counts, indices, limits, names, immediates, and instructions;
- control flow and memory addresses produced by validated WebAssembly execution;
- arguments supplied to exported WebAssembly functions;
- function references observed through imported tables unless they are proven to belong to the current instance.

Malformed or hostile module input must resolve to parser, validator, instantiation, or execution errors rather than relying on unchecked structural assumptions.

### Trusted extension boundary

Registered Rust host callbacks are trusted application extensions running in the same process. The runtime constrains their access to WebAssembly linear memory through explicit `HostCapabilities`, but it does not sandbox the callback's own Rust code, external I/O, CPU use, allocation behavior, or other process capabilities.

A callback with a granted capability is therefore authorized to create the corresponding side effects. Runtime validation of the callback's returned value is not a transaction or rollback mechanism.

## Security goals

### Fail closed on malformed structure

Parsing and validation happen before ordinary instantiation/execution. Unsupported or malformed module forms must be rejected rather than approximated.

The parser does not preallocate vectors directly from attacker-controlled binary counts. Tiny inputs that claim enormous vector lengths must fail while decoding missing entries instead of using the declaration alone to request proportional capacity.

### Bounded WebAssembly execution when limits are configured

`RuntimeLimits` provides independent defenses for:

- instruction fuel per exported invocation;
- host-call count per exported invocation;
- maximum WebAssembly call depth;
- maximum linear-memory pages.

Fuel is consumed before an instruction executes. In particular, exhausting fuel before a side-effecting instruction such as `memory.grow` must leave that side effect unapplied.

The limits are opt-in where their type is optional. Running with fuel or host-call limits disabled is not a claim of bounded execution time.

### Host-call budget precedes callback entry

The host-call budget is consumed before entering the registered callback. If the budget rejects a call, callback code has not executed and cannot have produced callback-side effects through the runtime context.

This ordering is security-sensitive and is covered by an adversarial regression test that also verifies module memory remains unchanged.

### Capabilities precede host memory access

`HostContext` checks memory-read or memory-write capability before exposing the requested operation. A callback may be entered without the capability, but the denied memory operation itself must not read or modify linear memory.

Bounds are checked before a permitted write copies any bytes. An out-of-bounds host memory write is therefore all-or-nothing with respect to WebAssembly linear memory.

### Shared-memory and shared-table initialization is preflighted

Active data and element segments are preflighted before mutating potentially host-shared imported memory or tables. A later invalid segment must not leave earlier initialization writes externally visible from a failed instantiation.

### Table references are instance-bound

Imported table function references carry instance identity. A stale or foreign function reference must not become callable merely because a numeric function index happens to exist in another instance.

### Runtime memory access is bounds checked

Supported WebAssembly load/store operations compute and validate effective ranges before reading or writing linear memory. Out-of-bounds operations trap rather than partially accessing memory.

## Deliberate non-transactional boundary: host callback results

Host callbacks execute before the runtime validates the arity and type of the returned value. If a callback has already performed an authorized side effect and then returns a value with the wrong arity or type, the runtime reports `HostResultArityMismatch` or `HostResultTypeMismatch` but does not roll back the callback's prior side effects.

This is deliberate current behavior and is covered by a regression test so documentation cannot accidentally imply stronger atomicity.

Applications that require transactional host operations must implement that transaction discipline inside the host integration layer rather than relying on WebAssembly result validation as a commit boundary.

## Resource-exhaustion boundary

The runtime uses checked allocation paths for parser count amplification and linear-memory/table creation where implemented, but it does not claim complete denial-of-service resistance.

Important residual boundaries include:

- `catch_unwind`-based mutation tests do not recover from process abort, OOM termination, stack exhaustion, or OS kills;
- a trusted host callback can run indefinitely or allocate independently of WebAssembly instruction fuel;
- no wall-clock deadline or preemptive execution interruption is provided;
- disabling fuel/host-call limits intentionally removes those execution bounds;
- the runtime is currently single-threaded and does not model shared-memory threads.

## Capability non-goals

The current runtime has no WASI implementation and grants no implicit filesystem, network, environment, clock, process, or operating-system capability to WebAssembly code.

However, calling a registered Rust host function is a transition into trusted application code. `HostCapabilities` currently gates the runtime's linear-memory access helpers; it is not a general-purpose capability system for arbitrary external resources used directly by the callback.

## Validation versus defense in depth

The validator is the primary structural/type gate. Runtime checks remain valuable defense in depth for dynamic conditions such as:

- memory bounds;
- indirect-call table bounds/null/type checks;
- host result validation;
- resource budgets;
- shared-object ownership constraints.

A runtime defensive error does not justify weakening validation of a condition that is statically knowable.

## Current executable security invariants

The Phase 6 security corpus explicitly locks these orderings:

1. host-call budget rejection happens before callback entry and before callback memory mutation;
2. denied host memory writes do not modify memory;
3. out-of-bounds host memory writes do not partially modify memory;
4. instruction fuel exhaustion before `memory.grow` prevents growth;
5. post-callback result validation does **not** roll back already-authorized callback effects.

These complement existing tests for call-depth limits, fuel exhaustion, host-call limits, memory limits, shared-import initialization atomicity, capability checks, and instance-bound table references.

## Out of scope / future hardening

This threat model does not claim:

- process-level sandboxing;
- protection from malicious native host callback code;
- wall-clock preemption;
- complete OOM or denial-of-service immunity;
- coverage-guided parser fuzzing;
- sanitizer-backed fuzzing;
- differential execution against a reference engine;
- JIT safety;
- WASI capability security;
- threads or shared-memory concurrency security.

The threat model must be revisited whenever new host capabilities, WASI-like interfaces, concurrency, multi-value execution, or a JIT changes the trust boundary.
