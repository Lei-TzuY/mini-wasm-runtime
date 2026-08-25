# Phase 5C pinned `call.wast` tranche

This tranche extends the committed WAST manifest with a source-faithful executable subset of `WebAssembly/spec` at pinned commit `fc209c5ed8afc4dfeb9252024d217da3376c7a6f` and hardens guest recursion so configured limits cannot overrun the interpreter's native stack safety ceiling.

## Provenance

- Upstream source: `test/core/call.wast`
- Pinned commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- Committed fixture: `crates/wasm-runtime/tests/fixtures/phase5c_upstream_call_subset.wast`
- Manifest accounting: 1 module, 65 executed assertions, 0 filtered directives

## Covered semantics

The selected upstream module exercises the currently supported direct-call surface across:

- i32/i64/f32/f64 parameters and results;
- ordered multi-value call results and argument/result permutation;
- direct recursion through depth 25 and branching recursion through Fibonacci depth 20;
- mutual-recursion base/small-depth cases while deeper upstream stress cases are treated as an implementation resource boundary;
- calls nested as operands of `select`, `if`, `br_if`, `br_table`, `call_indirect`, store/load, return, drop, branch, locals, globals, unary/binary/test/compare/conversion operators;
- the upstream `undefined element` indirect-call trap. The spec suite uses this trap text for both an out-of-bounds table selector and an uninitialized table element, so the WAST runner normalizes both runtime error variants into that spec trap class;
- a 100-argument mixed numeric call used by upstream to catch argument-passing/indexing defects.

## Host-stack safety hardening

The interpreter executes guest calls recursively in Rust. The previous default `max_call_depth` of 1024 allowed the native test thread stack to overflow before the guest limit fired. Staging probes showed that hard ceilings of 128 and then 64 were still too high for the interpreter's worst-case direct recursive frame on the hosted CI test-thread stack. The runtime therefore exposes `MAX_CALL_DEPTH = 32` as the validated native-stack safety ceiling. A configured lower limit is honored; a configured larger value is capped before recursive execution proceeds. A regression test sets `max_call_depth = usize::MAX` and requires a structured `RuntimeError::CallDepthExceeded` at the ceiling rather than host-process stack overflow.

This ceiling is an implementation safety bound, not a WebAssembly semantic limit. A future iterative guest-call-frame engine can raise or remove it without changing WebAssembly program semantics.

## Explicit exclusions

Seven executable directives from the upstream prefix are not selected:

1. two `assert_exhaustion` directives (`runaway` and `mutual-runaway`) because the current ingestion contract executes `assert_return` and `assert_trap`, not the distinct WAST exhaustion directive;
2. `as-memory.grow-value`, whose call result requests 306 additional pages and would test the repository's configurable memory-page policy as much as call semantics;
3. four mutual-recursion assertions requiring depths 77, 100, 200, and 77 (`even(100)`, `even(77)`, `odd(200)`, `odd(77)`), which cross the interpreter's native-stack safety ceiling.

The invalid-typing and unbound-function sections later in upstream `call.wast` remain outside this tranche because validation-failure directives are covered by the separate negative-conformance corpus. None of these exclusions are represented as runtime filters: the committed fixture contains only supported directives and must report exactly zero filtered directives.

## Accounting after this tranche

The pinned manifest contains 24 unique upstream sources and 705 selected executable assertions, with zero filters.

## Validation contract

The manifest test fails closed if provenance, module count, executed assertion count, fixture uniqueness, source uniqueness, or filtered-directive count drifts. This tranche is additionally validated with workspace formatting, Clippy with warnings denied, focused WAST ingestion and call-depth regression tests, full workspace tests, documentation builds, benchmark smoke, and Wasmtime differential smoke before merge.
