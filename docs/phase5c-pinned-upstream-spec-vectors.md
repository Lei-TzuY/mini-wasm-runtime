# Phase 5C — Pinned Upstream Spec Vectors

This slice adds a reproducible bridge between the runtime's existing hand-authored conformance tests and the upstream WebAssembly core spec tests.

## Pinned provenance

The upstream source is `WebAssembly/spec` commit:

`fc209c5ed8afc4dfeb9252024d217da3376c7a6f`

The commit is pinned deliberately. A floating `main` reference would allow upstream edits to change the claimed provenance without changing this repository.

The initial translated vectors are drawn from core numeric assertions such as `test/core/i32.wast` and corresponding i64 / floating-point / conversion coverage at that commit.

## Translation boundary

The repository does not yet parse WAST. Selected upstream assertions are translated into raw WebAssembly binaries by the test harness and then run through the public pipeline:

1. `wasm_parser::parse_module`
2. validation during `wasm_runtime::Instance::new`
3. `invoke_export`
4. assertion of the upstream-visible value or trap class

The translated fixtures do not call runtime numeric helpers directly. This keeps parser, validator, instantiation, dispatch, operand typing, and execution in the tested path.

## Initial vector families

The first pinned tranche covers already-supported semantics with high regression value:

- i32 wrapping add/sub/mul
- unsigned i32 division/remainder interpretation of high-bit operands
- divide-by-zero versus signed-overflow trap separation
- non-trapping signed remainder for `MIN % -1`
- modulo-width shift and rotate counts for i32 and i64
- signed-versus-unsigned comparison interpretation
- f32/f64 nearest ties-to-even and signed-zero preservation
- `i32.wrap_i64` and `i64.extend_i32_u` bit semantics

These vectors complement, rather than replace, the deterministic property corpus. Upstream vectors provide independent expected examples; property tests cover larger generated domains.

## Failure policy

A translated upstream vector is treated as conformance evidence, not a convenient expectation. If it exposes a mismatch:

- do not weaken or delete the vector merely to restore CI
- determine whether the runtime's supported-surface claim or the translation is wrong
- if the runtime is wrong, make the smallest semantic fix and retain the vector as regression coverage
- if the selected assertion depends on an unsupported proposal, remove it from this tranche only with an explicit scope explanation

Unsupported syntax/features continue to fail closed.

## What this does not claim

This is not complete upstream spec-test ingestion. It does not yet provide:

- a WAST parser or WAST command runner
- automatic extraction of `assert_return`, `assert_trap`, `assert_invalid`, or `assert_malformed`
- `register` / named-module cross-module scripts
- feature-aware automatic filtering of unsupported proposals
- bulk-memory/reference-types/thread/SIMD/memory64 coverage beyond the runtime's supported surface
- multi-value execution
- automatic upstream refreshes

The broader ingestion roadmap remains open until upstream `.wast` files can be consumed systematically with explicit feature filtering and reproducible provenance.
