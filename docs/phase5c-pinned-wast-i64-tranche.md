# Phase 5C pinned `i64.wast` tranche

This tranche extends the manifest-driven WAST ingestion corpus with a curated supported subset of `WebAssembly/spec` `test/core/i64.wast` pinned at commit `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`.

## Scope

The committed fixture exercises semantics already implemented by this runtime:

- wrapping i64 addition across signed boundaries,
- signed division divide-by-zero and `MIN / -1` overflow traps,
- signed division truncation toward zero,
- unsigned division over the high-bit-set i64 view,
- non-trapping `MIN % -1 == 0`, and
- signed remainder sign behavior for a negative dividend.

The manifest requires exact accounting of one module, eight executed assertions, and zero filtered directives. Any selected assertion becoming filtered or changing result/trap behavior fails the regression.

## Execution path

The fixture is parsed by the pinned test-only `wast` dependency, encoded to Wasm bytes, parsed by `wasm-parser`, validated and instantiated by the repository implementation, and executed through `Instance::invoke_export_values`. No reference runtime executes the assertions.

## Provenance and fail-closed behavior

The existing manifest guards remain unchanged: every row must use the exact pinned commit, identify a unique `test/core/*.wast` source and unique committed fixture, and the fixture itself must record both the pinned commit and declared source path. Exact module, executed-assertion, and filtered-directive counts are enforced per row.

This tranche does not claim support for complete `test/core/i64.wast`. In particular, it does not widen the runtime merely to accept unrelated instructions or proposal-era cases that are outside the currently supported surface.
