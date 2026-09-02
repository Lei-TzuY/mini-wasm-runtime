# Phase 5C pinned WAST manifest

The repository-committed manifest connects the systematic WAST runner to explicit upstream provenance and exact coverage accounting.

## Provenance contract

The manifest is `crates/wasm-runtime/tests/fixtures/phase5c_upstream_manifest.tsv` and pins `WebAssembly/spec` commit `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`. Each non-comment row records:

1. pinned upstream commit;
2. upstream `test/core/*.wast` source path;
3. committed curated fixture path;
4. expected encoded module count;
5. expected executed supported assertions;
6. expected executed bare invokes;
7. expected explicitly filtered directives.

The ingestion test rejects provenance drift, malformed rows, non-core source paths, invalid counts, duplicate source/fixture mappings, and fixture names that are not registered in the harness. CI does not fetch the upstream repository; committed fixtures are deterministic reviewed subsets whose source path and commit are provenance metadata.

## Consolidated baseline

At the 2026-09 consolidation point the manifest covers:

- 33 unique upstream core WAST sources;
- 1100 selected assertions;
- 4 stateful bare invokes;
- zero explicitly filtered directives.

The covered sources span function/control flow, i32/i64/f32/f64 numeric behavior, comparisons and conversions, locals/globals, memory/address/alignment/growth, direct and indirect calls, start behavior, and active data/element segments. The executable TSV remains authoritative for per-source module/assertion counts.

See `docs/consolidation-baseline-2026-09.md` for the frozen baseline and continuation policy.

## Coverage boundary

Each fixture is a curated supported subset, not a claim that the complete upstream source or the entire official WebAssembly test suite is accepted. Unsupported proposals, directives, and execution forms remain outside the current support boundary unless their semantics are explicitly implemented and reviewed.

Exact accounting is part of the contract. A change that silently drops a supported assertion, changes the pinned source, or introduces an unaccounted filter must fail rather than reducing coverage unnoticed.

## Expansion discipline

Pure conformance growth should now be organized around coherent upstream subsections or meaningful correctness boundaries rather than one pull request per handful of routine arithmetic vectors. Prefer reviewable batches when behavior is already implemented; keep smaller regression-first slices for traps, malformed inputs, stateful cases, or production discrepancies.

Adding assertions must not weaken validator checks, trap semantics, fail-closed behavior, or manifest accounting. If a run finds no distinct correctness value beyond a tiny routine-vector increment, no-op is preferable to creating another micro-PR.

Historical per-tranche documents remain provenance for how coverage arrived here. This consolidated contract is the current policy for maintaining the manifest.
