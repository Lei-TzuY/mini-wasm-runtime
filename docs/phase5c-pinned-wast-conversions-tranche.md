# Phase 5C pinned `conversions.wast` tranche

## Provenance

This tranche is a curated supported subset of `WebAssembly/spec` commit `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`, source `test/core/conversions.wast`. CI executes the committed fixture locally; it does not fetch a floating upstream branch.

## Covered semantics

The fixture exercises `i32.trunc_f32_s` and `i32.trunc_f32_u` through the existing WAST ingestion pipeline. The selected upstream assertions cover truncation toward zero, exact signed and unsigned boundaries, unsigned inputs in `(-1, 0)` truncating to zero, numeric overflow traps, and NaN producing `invalid conversion to integer` rather than numeric overflow.

The manifest requires exactly one module, twelve executed assertions, and zero filtered directives. Any selected assertion that becomes filtered or changes trap class fails the regression.

## Execution path

`wast` parses and encodes the committed text fixture. The encoded module is then parsed by `wasm-parser`, validated/instantiated by the repository runtime path, and invoked through the public values API. The WAST crate is only the script/text front end.

## Non-goals

This does not claim complete support for upstream `conversions.wast`. Promotion/demotion, reinterpret, integer-to-float, i64 truncation, and saturating-conversion cases remain covered by their existing focused tests and can be added to the pinned manifest in later reviewable tranches. No product opcode, validator rule, filter, or trap mapping is widened here.
