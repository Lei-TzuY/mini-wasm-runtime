# Roadmap

The roadmap favors complete vertical slices over a broad but shallow decoder.

## Phase 1 — callable integer MVP

- [x] module header
- [x] u32/i32 LEB128
- [x] type/function/export/code sections
- [x] structural validation
- [x] integer locals/constants/arithmetic
- [x] direct calls
- [x] CLI inspect/run
- [x] CI

## Phase 2 — validator and control flow

- [x] typed operand/control stacks for the i32-only subset
- [x] `block`, `loop`, `if`, `else`
- [x] `br`, `br_if`
- [ ] comparison/test instructions
- [x] unreachable/polymorphic stack rules
- [ ] negative conformance corpus

## Phase 3 — linear memory

- [x] memory section and limits
- [x] `i32.load` / `i32.store` families
- [x] bounds checks and trap model
- [x] `memory.size` / `memory.grow`
- [x] active data segments for memory 0

## Phase 4 — imports and host boundary

- [ ] import section
- [ ] host function registry
- [ ] typed host calls
- [ ] capability-oriented API
- [ ] resource limits

## Phase 5 — broader MVP + conformance

- [ ] globals
- [ ] tables and `call_indirect`
- [ ] i64/f32/f64 execution
- [ ] start/element/broader data modes
- [ ] WebAssembly spec tests for supported features

## Phase 6 — engineering hardening

- [ ] parser fuzzing
- [ ] property-based tests
- [ ] differential execution against a reference engine in tests only
- [ ] deterministic benchmarks
- [ ] malformed-module corpus
- [ ] security invariants and threat model

A future JIT is intentionally out of scope until the interpreter and validation model are trustworthy.
