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
- [x] defined-function and structured-control multi-value results
- [x] host callback multi-result ABI with backward-compatible zero-or-one-result registration
- [x] MVP `nop`, `drop`, `select`, and `br_table` control/parametric instructions
- [x] MVP i32/i64 count, div/rem, bitwise, shift, and rotate operators
- [x] core sign-extension integer operators (`i32.extend8_s`, `i32.extend16_s`, `i64.extend8_s`, `i64.extend16_s`, `i64.extend32_s`)
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
- [x] pinned `load.wast` and `store.wast` instruction-composition manifest tranche with exact accounting
- [x] pinned `address.wast` and `align.wast` memory-boundary/alignment manifest tranche with exact accounting
- [x] pinned `call.wast` direct-call/recursion/composition manifest tranche with exact accounting
- [x] pinned `call_indirect.wast` typing/dispatch/structural-equivalence manifest tranche with exact accounting
- [x] pinned `local_get.wast`, `local_set.wast`, and `local_tee.wast` local-state/composition manifest tranche with exact accounting
- [x] pinned `global.wast` numeric-global/imported-state/composition manifest tranche with exact accounting
- [x] MVP `unreachable` trap execution plus pinned `unreachable.wast` control/composition manifest tranche with exact accounting
- [x] complete pinned `start.wast` validation/start-trap/spectest-import/stateful-invoke/quoted-malformed manifest tranche with phase-sensitive exact accounting
- [x] pinned `data.wast` active-data boundary/instantiation-trap manifest tranche with phase-sensitive exact accounting
- [x] pinned `elem.wast` defined/imported-table active-element boundary/instantiation-trap manifest tranche with phase-sensitive exact accounting
- [x] pinned `block.wast` structured-control/parameter manifest tranche with exact accounting
- [x] pinned `loop.wast` parameter/label manifest tranche with exact accounting
- [x] pinned `if.wast` result/parameter/branch manifest tranche with exact accounting
- [x] pinned `i64.wast` arithmetic/div/rem manifest tranche with exact accounting
- [x] pinned `f32.wast` arithmetic/rounding manifest tranche with exact accounting
- [x] pinned `f64.wast` arithmetic/rounding manifest tranche with exact accounting
- [x] pinned `f32_cmp.wast` comparison/NaN manifest tranche with exact accounting
- [x] pinned `f64_cmp.wast` comparison/NaN manifest tranche with exact accounting
- [x] pinned `conversions.wast` f32-to-i32 trapping-conversion manifest tranche with exact accounting
- [x] pinned `nop.wast`, `select.wast`, and `br_table.wast` parametric/control manifest tranche with exact accounting
- [ ] expand pinned upstream WAST manifest coverage across the remaining supported numeric/control/memory surface
- [x] initial negative-conformance corpus for the supported surface
- [ ] continue adversarial corpus expansion as new surfaces land

## Phase 6 — engineering hardening

- [x] initial cargo-fuzz parser and parse-to-validation targets with bounded nightly CI smoke
- [x] scheduled coverage-guided campaigns, corpus minimization, sanitizer-backed fuzzing, and source-coverage report automation
- [x] promote coverage blind spots and real fuzz discoveries into reviewed deterministic seeds/regressions
- [x] initial deterministic property-based / metamorphic corpus
- [x] deterministic shrinking and initial structured generated-property domains
- [x] broaden structured generators to multi-value, tables, imports, and richer stateful memory sequences
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
- [x] imported host-function trace mismatch capture with sequence/state/salt/input shrinking and driver-complete artifacts
- [x] reviewed import-aware replay/promotion manifest for stateful imported host-function captures
- [x] imported mutable-global host-override mismatch capture/shrinking plus reviewed replay manifest
- [x] imported memory host-override mismatch capture/shrinking plus reviewed replay manifest
- [x] imported funcref-table host-mutation mismatch capture/shrinking plus reviewed replay manifest
- [x] stable typed host-failure normalization across callback rejection, capability denial, unavailable memory, and host-memory OOB
- [x] initial deterministic interpreter benchmark workloads and smoke harness
- [x] controlled-host baseline capture/comparison tooling with median/MAD noise-aware regression policy
- [ ] record the first reviewed baseline on a pinned controlled host and operationalize periodic performance checks
- [x] initial malformed-binary parser corpus
- [x] untrusted-count parser allocation hardening
- [x] initial malformed-module validation/runtime stage corpus
- [x] expand stage-sensitive malformed-module coverage across import/export index spaces, host bindings, active segments, and normalized dynamic traps
- [ ] continue malformed-module corpus expansion from fuzzing and differential regressions
- [x] initial runtime security invariants and threat model
- [ ] revisit the threat model as host capabilities, concurrency, WASI-like interfaces, or JIT execution expand

## Phase 7 — bounded WASI Preview1 host surface

The runtime has moved beyond a generic future-WASI placeholder into an executable, capability-scoped Preview1 subset. This phase expands that subset one bounded host contract at a time while preserving deterministic tests, explicit resource limits, and fail-closed guest-memory preflight.

- [x] bounded `fd_write` for stdout/stderr with iovec gathering and atomic host-output commit
- [x] bounded `fd_fdstat_get` metadata for standard descriptors
- [x] deterministic process arguments via `args_sizes_get` / `args_get`
- [x] deterministic process environment via `environ_sizes_get` / `environ_get`
- [x] deterministic stdin plus bounded `fd_read` scatter writes, sequential consumption, EOF, and read rights
- [x] process termination semantics (`proc_exit`) with immediate non-local guest termination and a typed non-error WASI invocation outcome
- [x] deterministic injected entropy/time capabilities (`random_get`, `clock_res_get`, and `clock_time_get`) without ambient host nondeterminism
- [x] bounded preopen directory discovery via `fd_prestat_get` / `fd_prestat_dir_name` with deterministic allocation, immutable guest namespace names, and fixed configuration bounds
- [x] bounded injected read-only path descriptors via `path_open` -> `fd_read` -> `fd_close` with `PATH_OPEN` / `FD_READ` rights attenuation, traversal-safe relative paths, bounded dynamic descriptor allocation/reuse, and no ambient host filesystem access
- [x] read-only descriptor positioning via `fd_seek` / `fd_tell` with one shared `u64` cursor, Preview1 `SET` / `CUR` / `END` semantics, `FD_SEEK` / `FD_TELL` rights attenuation, seek-beyond-EOF preservation, and fail-closed invalid/overflow/OOB handling
- [x] cursor-preserving positioned regular-file reads via `fd_pread` with `FD_READ` + `FD_SEEK` attenuation, explicit `u64` offsets, bounded scatter writes, EOF/partial-read behavior, shared read limits, and fail-closed guest-memory preflight
- [x] stable regular-file metadata identity via `fd_filestat_get` with `FD_FILESTAT_GET` attenuation, deterministic synthetic device/inode identity, live shared size, fixed logical-epoch timestamps, and 64-byte fail-closed guest-memory preflight
- [x] bounded regular-file resizing via `fd_filestat_set_size` with `FD_FILESTAT_SET_SIZE` attenuation, shrink/zero-fill extension semantics, cursor preservation, writable-file policy enforcement, and the existing fixed 16 MiB file-size ceiling
- [x] capability-scoped writable/create filesystem semantics with writable preopens, bounded `path_open(O_CREAT)`, sequential cursor-based `fd_write`, cursor-preserving `fd_pwrite`, sparse zero-fill, rights attenuation, and no ambient host paths
- [x] initial WASI-specific differential/interop evidence for deterministic process arguments and environment against pinned Wasmtime-WASI 37.0.3, comparing errno results and exact guest-memory layouts
- [x] deterministic WASI descriptor I/O differential/interop evidence against pinned Wasmtime-WASI 37.0.3 for sequential stdin reads, EOF, `nread` / `nwritten`, exact guest-memory state, and captured stdout
- [ ] broaden WASI differential/interop coverage to clocks/entropy with controllable reference providers and filesystem semantics where host resources can be isolated deterministically

A future JIT is intentionally out of scope until the interpreter and validation model are trustworthy.
