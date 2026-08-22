# Phase 5C — Pinned `return.wast` Typed/Memory Tranche

This tranche extends the existing pinned `WebAssembly/spec` `test/core/return.wast` fixture with source-faithful contexts that exercise WebAssembly unreachable-stack polymorphism through already-supported typed numeric and memory instructions.

## Provenance

- upstream repository: `WebAssembly/spec`
- pinned commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- upstream source: `test/core/return.wast`
- committed fixture: `phase5c_upstream_return_subset.wast`

The previous tranche contained 31 selected assertions. This tranche adds 13 assertions without widening the admitted instruction surface.

## Semantic invariant

A `return` immediately transfers control to the current function label. Code in the enclosing expression after that transfer is unreachable, so validation must apply WebAssembly's polymorphic-stack rules rather than require the returned value to satisfy an operand type that will never be consumed at runtime.

The added cases therefore test two things at once:

1. validation accepts otherwise type-incompatible enclosing operand positions after `return` makes them unreachable;
2. execution exits before the enclosing numeric, comparison, or memory instruction can observe or mutate state.

## Selected upstream contexts

The added assertions cover:

- typed unreachable-result composition through `i32.ctz`, `i64.ctz`, `f32.neg`, and `f64.neg`;
- memory operand positions for `f32.load`, `i64.load8_s`, `f64.store`, `i64.store`, `i32.store8`, and `i64.store16`;
- unary `f32.neg`;
- comparison operands for `f64.le` and `f32.ne`.

These are intentionally chosen from contexts whose transitive opcodes are already supported. Upstream cases requiring unrelated unsupported `drop`, `nop`, `select`, or `br_table` remain excluded instead of changing product semantics for corpus breadth.

## Exact accounting

The single `return.wast` manifest row changes from:

- 1 module
- 31 executed assertions
- 0 filtered directives

to:

- 2 modules
- 44 executed assertions
- 0 filtered directives

Across the sixteen unique pinned upstream source rows, selected assertions increase from 359 to 372. Exact accounting remains fail-closed: if a selected directive becomes filtered, a module fails to execute, or return/unreachable semantics drift, the manifest regression fails.

## Validation boundary

The fixture/manifest-only semantic candidate at `62931a7834781f1b1f0143ce1a255f2854c35571` passed the complete GitHub Actions matrix:

- Rust stable / Ubuntu
- Rust stable / Windows
- Rust stable / macOS
- Rust 1.81.0 / Ubuntu

Ubuntu logs explicitly confirmed `phase5c_wast_ingestion` 3/3 and `pinned_upstream_manifest_executes_with_exact_accounting` passing with the expanded row. Final validation is rerun on the documentation-inclusive HEAD before the PR is sealed.

## Non-goals

This tranche does not change:

- parser, validator, interpreter, or host ABI behavior;
- WAST filtering, expected-value matching, or trap mapping;
- dependencies, workflow, warning policy, or CI acceptance;
- unsupported `drop`, `nop`, `select`, or `br_table` instructions;
- the roadmap completion state for broader upstream WAST coverage.

It does not claim complete `return.wast` support.
