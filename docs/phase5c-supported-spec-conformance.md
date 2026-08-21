# Phase 5C Supported-Spec Conformance Slice

This slice adds a small, explicit conformance harness for semantics that are already inside mini-wasm-runtime's supported WebAssembly surface. It does not claim to run the upstream WebAssembly spec repository wholesale and does not broaden the accepted language.

## Contract

Each fixture constructs a valid binary module, passes it through the public parser and `Instance::new` validation/instantiation path, invokes an exported function, and asserts the spec-visible result or trap class.

The harness intentionally exercises behavior through the public runtime boundary instead of calling numeric helper functions directly. A parser, validator, decoder, or runtime regression can therefore fail the same case.

## Initial cases

The initial translated semantic cases cover:

- signed `i32.div_s` and `i32.rem_s` truncation toward zero
- modulo-32 masking of `i32.shl` shift counts
- deterministic signed-zero selection for `f32.min` and `f32.max`
- the distinction between NaN invalid-conversion traps and numeric-overflow traps for `i32.trunc_f64_s`
- valid unsigned truncation of a negative fraction whose truncated value is zero
- `memory.grow` returning `-1` when the declared maximum prevents growth

These cases overlap some lower-level regression tests by design. Their value is the stable, end-to-end assertion shape that future spec-derived cases can reuse.

## Fail-closed boundary

This slice does not add parser acceptance, validator exceptions, runtime fallback behavior, dependency-based reference execution, or warning suppression. Unsupported proposal features remain rejected exactly as before.

## Non-goals

- vendoring or automatically consuming the full upstream `.wast` corpus
- implementing a WAT/WAST parser
- multi-value execution
- bulk memory/table instructions, SIMD, memory64, multi-memory, or multi-table
- treating this initial corpus as complete spec-test conformance

A later slice can either expand these translated cases or add a dedicated upstream-spec ingestion strategy after provenance, feature filtering, and unsupported-assertion handling are designed explicitly.
