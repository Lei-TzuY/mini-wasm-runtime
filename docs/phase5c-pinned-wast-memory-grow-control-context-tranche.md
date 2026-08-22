# Phase 5C — Pinned `memory_grow.wast` Control-Context Tranche

This tranche extends the existing pinned `WebAssembly/spec` `test/core/memory_grow.wast` fixture with source-faithful uses of `memory.grow` as an operand of control, call, state-update, and nested-growth constructs that are already supported by the runtime.

## Provenance

- upstream repository: `WebAssembly/spec`
- pinned commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- upstream source: `test/core/memory_grow.wast`
- committed fixture: `phase5c_upstream_memory_grow_subset.wast`

## Selected supported contexts

Twelve assertions cover:

- `memory.grow` as a `br` result value
- `memory.grow` as a `br_if` condition
- `memory.grow` as a `return` value
- `memory.grow` as an `if` condition, then result, and else result
- `memory.grow` as the first, middle, and last argument to a direct call
- `memory.grow` as the value consumed by `local.set`
- `memory.grow` as the value consumed by `global.set`
- nested `memory.grow (memory.grow 0)`

These vectors exercise value-stack/control-stack composition while preserving the instruction's stateful contract: successful growth returns the previous page count, and zero growth must not change memory size.

## Adversarial candidate finding

The first semantic candidate also selected upstream `as-br_if-value` and `as-br_if-value-cond`. CI rejected that candidate with `UnsupportedOpcode` `0x1a` (`drop`). Those functions depend transitively on `drop`, which is outside the currently supported product surface.

The failure was treated as a useful fail-closed signal. The runtime, validator, WAST runner, and filter policy were not widened. The two `drop`-dependent cases were replaced with source-faithful `local.set` and `global.set` contexts from the same pinned upstream module, preserving twelve selected assertions and zero filters.

The upstream control-context module also contains `br_table` and `select` cases. They remain excluded for the same reason: conformance breadth must not silently widen product behavior.

## Exact accounting

The existing `memory_grow.wast` manifest row changes from:

- 2 modules
- 28 executed assertions
- 0 filtered directives

to:

- 3 modules
- 40 executed assertions
- 0 filtered directives

Across the pinned manifest, selected assertions increase from 234 to 246. Exact manifest accounting remains fail closed: any selected directive becoming filtered, failing validation/instantiation, or producing a different result fails the regression.

## Validation boundary

The corrected fixture/manifest-only semantic candidate passed the complete GitHub Actions matrix before this document was added:

- Rust stable / Ubuntu
- Rust stable / Windows
- Rust stable / macOS
- Rust 1.81.0 / Ubuntu

Ubuntu logs explicitly confirmed `phase5c_wast_ingestion` and `pinned_upstream_manifest_executes_with_exact_accounting` passing with the 3-module / 40-assertion `memory_grow.wast` row. Final validation is rerun on the documentation-inclusive HEAD before the PR is sealed.

## Non-goals

This tranche does not change parser, validator, interpreter, host ABI, WAST runner/filter logic, dependencies, workflows, warning policy, or CI acceptance. It does not claim complete upstream `memory_grow.wast` support; contexts that transitively require unsupported instructions remain outside the curated supported subset.
