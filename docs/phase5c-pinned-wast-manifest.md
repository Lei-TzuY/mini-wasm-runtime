# Phase 5C pinned WAST manifest

This slice connects the existing systematic WAST runner to an explicit, repository-committed upstream provenance manifest.

## Provenance contract

The manifest pins WebAssembly/spec commit `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`. Each non-comment row records:

1. pinned upstream commit;
2. upstream `test/core/*.wast` source path;
3. committed curated fixture path;
4. expected encoded module count;
5. expected executed supported assertions;
6. expected explicitly filtered directives.

The test rejects rows that drift from the pinned commit, malformed rows, non-core `.wast` source paths, invalid counts, or fixture names that are not explicitly registered in the harness.

## Current tranche

The first manifest row maps `test/core/func.wast` to a committed supported subset covering ordered multi-value export, `return`, and branch-to-function-label result vectors. The runner parses the text with the pinned `wast` dev dependency, encodes it, then executes it through this repository's parser, validator, instantiator, and public invocation API.

Expected accounting is exact: one module, three executed assertions, zero filtered directives. A change that silently skips a supported assertion therefore fails the test instead of reducing coverage unnoticed.

## Boundaries

The committed fixture is a curated supported subset, not a claim that the complete upstream file or full official test suite is accepted. Unsupported upstream directives and feature proposals remain outside the current support boundary and must be added only with explicit filter/accounting expectations.

No production parser, validator, runtime, host ABI, dependency, or CI policy changes are part of this slice.
