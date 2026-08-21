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

- [x] function import section
- [x] combined imported/defined function index space
- [x] host function registry
- [x] typed host calls and import resolution
- [x] capability-oriented host context
- [x] bounded host memory read/write access
- [x] configurable call-depth and memory limits
- [x] instruction fuel and host-call budgets

## Phase 5 — broader MVP + conformance

### Phase 5A — state, tables, and indirect calls

- [x] defined i32 globals
- [x] `global.get` / `global.set`
- [x] funcref table section and limits
- [x] active table-0 element segments
- [x] `call_indirect` with bounds, null, and dynamic type traps
- [x] start section and `[] -> []` start execution
- [x] table/global exports

### Phase 5B — broader numeric execution

- [ ] i64 execution
- [ ] f32/f64 execution
- [ ] comparison/test instructions
- [ ] broader numeric conversions

### Phase 5C — broader module forms + conformance

- [ ] broader data/element modes
- [ ] table/memory/global imports
- [ ] block parameters and type-index block signatures
- [ ] WebAssembly spec tests for supported features

## Phase 6 — engineering hardening

- [ ] parser fuzzing
- [ ] property-based tests
- [ ] differential execution against a reference engine in tests only
- [ ] deterministic benchmarks
- [ ] malformed-module corpus
- [ ] security invariants and threat model

A future JIT is intentionally out of scope until the interpreter and validation model are trustworthy.
