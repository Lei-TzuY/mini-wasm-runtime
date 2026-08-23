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
- [x] initial cross-layer negative-conformance corpus (completed in Phase 5C)

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
- [x] broader data/element modes
- [x] defined-function and structured-control multi-value results; host imports remain zero-or-one result
- [x] MVP `nop`, `drop`, `select`, and `br_table` control/parametric instructions
- [x] MVP i32/i64 count, div/rem, bitwise, shift, and rotate operators
- [x] MVP f32/f64 unary, arithmetic, min/max, and copysign operators
- [x] bit-exact i32/f32 and i64/f64 reinterpret instructions
- [x] unprefixed trapping float-to-integer and integer-to-float conversions
- [x] saturating float-to-integer conversions (`0xfc` prefix)
- [x] i64/f32/f64 memory instruction families
- [x] initial end-to-end spec-derived conformance corpus for supported semantics
- [x] pinned upstream-spec translated-vector tranche for supported numeric and multi-value semantics
- [x] WAST parser/filter/runner ingestion infrastructure for supported core assertions
- [x] manifest-driven pinned upstream WAST subset with exact executed/filtered accounting
- [x] pinned `i32.wast` arithmetic/trap manifest tranche with duplicate/provenance guards
- [x] pinned `memory.wast` narrow load/store manifest tranche with exact accounting
- [x] pinned `block.wast` structured-control/parameter manifest tranche with exact accounting
- [x] pinned `loop.wast` parameter/label manifest tranche with exact accounting
- [x] pinned `if.wast` result/parameter/branch manifest tranche with exact accounting
- [x] pinned `i64.wast` arithmetic/div/rem manifest tranche with exact accounting
- [x] pinned `f32.wast` arithmetic/rounding manifest tranche with exact accounting
- [x] pinned `f64.wast` arithmetic/rounding manifest tranche with exact accounting
- [x] pinned `f32_cmp.wast` comparison/NaN manifest tranche with exact accounting
- [x] pinned `f64_cmp.wast` comparison/NaN manifest tranche with exact accounting
- [x] pinned `conversions.wast` f32-to-i32 trapping-conversion manifest tranche with exact accounting
- [ ] expand pinned upstream WAST manifest coverage across the remaining supported numeric/control/memory surface
- [x] initial negative-conformance corpus for the supported surface
- [ ] continue adversarial corpus expansion as new surfaces land

## Phase 6 — engineering hardening

- [x] initial cargo-fuzz parser and parse-to-validation targets with bounded nightly CI smoke
- [ ] long-running coverage-guided campaigns, corpus minimization, and sanitizer/coverage automation
- [x] initial deterministic property-based / metamorphic corpus
- [x] deterministic shrinking and initial structured generated-property domains
- [ ] broaden structured generators to multi-value, tables, imports, and richer stateful memory sequences
- [x] deterministic parser/validator mutation robustness corpus
- [x] initial Wasmtime differential execution corpus in an isolated test workspace
- [x] deterministic differential module generation and initial exact trap-class normalization
- [x] initial table/indirect-call trap normalization and stateful global/memory differential generation
- [x] generated table-dispatch state transitions and structured multi-value differential cases
- [x] imported mutable-global/memory shared-state differential cases, including cross-instance aliasing
- [x] initial minimized seeded differential regression replay corpus
- [x] imported host-function state/ABI differentials, including cross-instance shared callback state
- [x] imported table dispatch, host-mutation, null-trap, and limit-matching differentials
- [x] host callback guest-memory read/write differentials plus fail-closed capability and bounds guards
- [x] imported callback failure normalization with typed Wasmtime error downcast and post-trap recovery/state checks
- [x] initial automatic reference-backed mismatch shrinking and CI capture artifacts for generated i32 cases
- [x] memory value/OOB mismatch capture with boundary-aware address/offset/value shrinking
- [x] structured multi-value mismatch capture with branch/value shrinking and replay-ready tuples
- [x] table result/null/OOB mismatch capture with selector/initializer/value shrinking
- [ ] broaden mismatch shrinking/reviewed promotion to imports and additional stable typed host failures
- [x] initial deterministic interpreter benchmark workloads and smoke harness
- [ ] establish controlled-host baselines and a performance regression policy
- [x] initial malformed-binary parser corpus
- [x] untrusted-count parser allocation hardening
- [x] initial malformed-module validation/runtime stage corpus
- [ ] continue malformed-module corpus expansion from fuzzing and differential regressions
- [x] initial runtime security invariants and threat model
- [ ] revisit the threat model as host capabilities, concurrency, WASI-like interfaces, or JIT execution expand

A future JIT is intentionally out of scope until the interpreter and validation model are trustworthy.
