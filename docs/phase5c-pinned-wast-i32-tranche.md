# Phase 5C — Pinned i32 WAST Manifest Tranche

This tranche extends the manifest-driven WAST ingestion introduced by PR #29 with a second curated source selection from the same fixed upstream WebAssembly specification commit.

## Provenance

The manifest remains pinned to `WebAssembly/spec` commit `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`.

The new row names `test/core/i32.wast` and maps it to the committed fixture `phase5c_upstream_i32_subset.wast`. The fixture is a curated supported subset, not a verbatim copy of the complete upstream file.

CI does not fetch the upstream repository. The source path and commit are provenance metadata for a repository-committed deterministic fixture.

## Selected semantics

The i32 subset contains one module and seven assertions covering:

- wrapping addition across signed boundaries,
- signed division by zero,
- signed `MIN / -1` overflow,
- signed division truncation toward zero,
- non-trapping `MIN % -1 == 0`, and
- signed remainder behavior for a negative dividend.

The manifest records exact accounting of one module, seven executed assertions, and zero filtered directives.

## Manifest integrity

With more than one manifest row, coverage accounting now also rejects duplicate upstream source paths and duplicate fixture names. Before execution, every registered fixture must contain both the exact pinned commit and the upstream source path named by its manifest row.

These checks prevent an accidental duplicate row, stale provenance comment, or fixture/source mismatch from inflating apparent coverage while remaining green.

## Execution path

The curated WAST is still processed through the PR #28/#29 pipeline:

1. parse WAST using the pinned dev-only `wast` parser,
2. encode the text module to WebAssembly binary,
3. parse that binary with this repository's parser,
4. validate and instantiate with this repository's implementation, and
5. execute assertions through the public runtime export API.

The upstream parser is only the test-script/text front end; it is not a reference execution engine.

## Scope and non-goals

This tranche does not claim that complete `test/core/i32.wast` passes, nor that the official WebAssembly spec suite is supported. Unsupported upstream assertions are not copied into the curated fixture and therefore are not counted as passing or filtered.

No production parser, validator, runtime, host ABI, dependency, or CI policy changes are made. Broader manifest expansion across supported numeric, control-flow, and memory semantics remains separate follow-up work.
