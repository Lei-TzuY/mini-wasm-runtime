# Bounded memory64 address-width slice

This slice adds an initial executable subset of the WebAssembly memory64 proposal while preserving the runtime's existing bounded-allocation model.

## Implemented behavior

- memory types decode the memory64 address-width bit and use 64-bit page limits;
- memory32 limits remain bounded to the 32-bit memory page ceiling;
- memory64 validation accepts the proposal's 48-bit page-count ceiling while instantiation still applies the embedder/runtime physical page cap before allocation;
- scalar memory loads and stores use `i64` addresses for memory64 and continue to use `i32` addresses for memory32;
- scalar memarg displacements are decoded as `u64`, with memory32 validation rejecting displacements outside its 32-bit address width;
- memory64 `memory.size` returns `i64`;
- memory64 `memory.grow` consumes and returns `i64`, including the all-ones failure result;
- existing multi-memory indices continue to select the memory whose address width determines instruction typing;
- addresses that are valid in the proposal's 64-bit address space but outside this runtime's bounded physical backing trap fail-closed rather than being truncated.

## Evidence

Deterministic parser, validator, runtime, malformed-module, security-invariant, and host-boundary regressions exercise memory32 preservation and the memory64 address-width boundary. The isolated differential workspace enables memory64 in pinned Wasmtime and compares memory sizing/growth, i64-addressed scalar access, and a memarg displacement above `u32::MAX` that is valid to encode for memory64 but traps at runtime with the current bounded backing.

## Deliberate boundaries

This is an address-width vertical slice, not a claim of complete memory64 conformance. The following remain separate capabilities:

- memory64 bulk-memory `memory.init`, `memory.copy`, and `memory.fill` address typing;
- imported memory64 host backing;
- larger physical allocations beyond the runtime's existing configured memory-page ceiling;
- shared-memory/threads combinations;
- additional upstream memory64 WAST manifest accounting beyond the focused reference differential.

Unsupported combinations continue to fail closed. These boundaries should be expanded as separate coherent vertical slices with their own deterministic and reference-backed evidence.
