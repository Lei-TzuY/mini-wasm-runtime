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

- [x] typed operand/control stacks for the original i32 subset
- [x] `block`, `loop`, `if`, `else`
- [x] `br`, `br_if`
- [x] core integer comparison/test instructions (completed in Phase 5B)
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

- [x] defined globals
- [x] `global.get` / `global.set`
- [x] funcref table section and limits
- [x] active table-0 element segments
- [x] `call_indirect` with bounds, null, and dynamic type traps
- [x] start section and `[] -> []` start execution
- [x] table/global exports

### Phase 5B — typed numeric core

- [x] replace arity-only validation with one true typed operand stack
- [x] i32/i64/f32/f64 defined-function params, locals, results, globals, and block results
- [x] i64 constants and wrapping add/sub/mul
- [x] f32/f64 constants and add/sub/mul/div
- [x] i32/i64 signed/unsigned comparisons and `eqz`
- [x] f32/f64 comparisons with IEEE NaN behavior
- [x] selected non-trapping numeric conversions
- [x] typed direct and indirect calls for defined functions
- [x] typed runtime argument/control/global defense-in-depth checks
- [x] typed CLI values

### Phase 5C — broader module forms + conformance

- [x] parser descriptors for function/table/memory/global imports
- [x] independent function/table/memory/global index-space accounting
- [x] immutable numeric global imports with explicit host binding
- [x] shared backing for mutable global imports
- [x] shared backing for table imports with instance-bound function references
- [x] shared backing for memory imports with runtime-limit-safe shared linear-memory state
- [x] non-i32 host function import ABI
- [x] block parameters and type-index block signatures with zero-or-one result
- [ ] broader data/element modes
- [ ] multi-value results
- [ ] broader numeric operators, reinterpret, and trapping conversions
- [x] i64/f32/f64 memory instruction families
- [ ] WebAssembly spec tests for supported features
- [ ] negative conformance corpus

## Phase 6 — engineering hardening

- [ ] parser fuzzing
- [ ] property-based tests
- [ ] differential execution against a reference engine in tests only
- [ ] deterministic benchmarks
- [ ] malformed-module corpus
- [ ] security invariants and threat model

A future JIT is intentionally out of scope until the interpreter and validation model are trustworthy.
