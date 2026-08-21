# Phase 5C — Pinned Upstream Spec Vectors

This tranche translates a deliberately small set of upstream WebAssembly core assertions into the runtime's existing raw-binary integration harness.

## Pinned provenance

The upstream source is fixed to `WebAssembly/spec` commit:

`fc209c5ed8afc4dfeb9252024d217da3376c7a6f`

The pin is intentional. CI behavior must not change merely because upstream `main` advances.

The translated assertions currently come from the pinned core tests for integer operations/comparisons, floating-point rounding, conversions, `func.wast`, and `call.wast`.

## Why this PR is restacked

The original pinned-vector PR #25 and multi-value PR #26 were created concurrently from the same PR #24 base. They are siblings, not ancestors. Retargeting #25 directly onto #26 would therefore create a misleading reverse-diff against changes that only exist on #26.

This branch starts from the final #26 head and ports the supported vectors forward as a clean additive delta. PR #25 can be closed unmerged once this replacement is fully validated.

## Translation boundary

There is no WAST parser in this tranche. Each selected upstream assertion is translated into a minimal raw WebAssembly binary that exercises the same supported semantic rule through public APIs:

1. `wasm_parser::parse_module`
2. runtime validation/instantiation through `Instance::new`
3. `Instance::invoke_export` for zero-or-one-result vectors
4. `Instance::invoke_export_values` for multi-result vectors
5. exact value, raw-bit, or runtime-trap assertion

Unsupported upstream syntax or proposals are not silently approximated. A vector is included only when the current implementation can represent its semantics faithfully.

## Carried numeric vectors

The restacked tranche preserves the six test groups from PR #25:

- i32 wrapping add/sub/mul and unsigned div/rem views
- exact divide-by-zero versus signed-overflow trap classes
- non-trapping signed `MIN % -1`
- i32/i64 shift and rotate count masking
- signed-versus-unsigned comparison views
- f32/f64 `nearest` ties-to-even and negative-zero preservation
- `i32.wrap_i64` and `i64.extend_i32_u` bit semantics

## Multi-value vectors enabled by PR #26

The replacement additionally translates selected pinned assertions that were previously out of scope:

From `test/core/func.wast`:

- `value-i32-f64`: ordered `[i32, f64]` export results
- `value-i32-i32-i32`: ordered three-value export results
- `value-block-i32-i64`: multi-result block convergence
- `return-i32-f64`: multi-result return propagation
- `break-i32-f64`: branch-to-function-label result propagation

From `test/core/call.wast`:

- `$const-i32-i64` through exported `type-i32-i64`: direct calls forward the entire result vector in order

These cases intentionally use `invoke_export_values`; they do not weaken the legacy zero/one-result API boundary introduced by PR #26.

## Failure policy

A pinned vector is an external conformance constraint, not a test to be adjusted until green. If one fails:

1. verify the raw-binary translation against the pinned upstream assertion;
2. if translation is wrong, fix the translation without weakening the assertion;
3. if runtime/parser/validator behavior is wrong, keep the vector and make the smallest product fix;
4. preserve fail-closed behavior for unsupported semantics.

## Non-goals

This tranche does **not** claim:

- full WebAssembly spec-suite coverage;
- automatic `.wast` parsing or execution;
- supported-feature discovery/filtering across the upstream suite;
- automatic upstream refresh;
- host-callback multi-result ABI support;
- SIMD, memory64, multi-memory, multi-table, threads, GC, or other unsupported proposals.

The next conformance step should be systematic pinned `.wast` ingestion with an explicit supported-feature filter, rather than continuing to hand-translate isolated vectors indefinitely.
