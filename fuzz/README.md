# Parser fuzzing

This directory is an isolated `cargo-fuzz` workspace. It does not change the product workspace's Rust 1.81 MSRV or add a production runtime dependency.

## Targets

- `parse_module`: feeds arbitrary bytes directly to `wasm_parser::parse_module` and treats any panic/abort surfaced by libFuzzer as a bug.
- `parse_validate`: parses arbitrary bytes and, only when parsing succeeds, passes the resulting module into `wasm_validator::validate`. This stresses the parser-to-validator trust boundary without invoking arbitrary code.

## Reviewed seed corpus

`fuzz/seeds/manifest.tsv` is the reviewable source of truth for committed fuzz seeds. Every row has five fields:

```text
id    targets    expectation    hex    note
```

`targets` is a comma-separated subset of `parse_module,parse_validate`. `expectation` is one of `valid`, `validation-error`, or `parse-error`. The initial corpus deliberately reaches successful validation, parser-success/validator-rejection paths, and parser rejection across functions/code, multi-value results, imports, memory/data, globals, tables/elements/start, malformed LEB input, duplicate sections, invalid exports, and the current single-memory boundary.

The binary `fuzz/corpus/<target>/` directories remain generated/ignored data. Materialize the reviewed text seeds into either target with:

```text
python3 fuzz/materialize-seeds.py parse_module
python3 fuzz/materialize-seeds.py parse_validate
```

The materializer validates the manifest, rejects unsafe/duplicate IDs and duplicate payloads, hashes existing corpus contents, and only adds missing reviewed seeds. It never deletes an evolving corpus restored from cache.

The same manifest is replayed by `crates/wasm-validator/tests/fuzz_seed_replay.rs`. That deterministic test independently decodes every committed hex payload and requires its parser/validator stage classification to remain exactly stable. A seed cannot silently become valid, become invalid at a different trust boundary, or disappear from both success and rejection coverage.

## Corpus review and promotion

`crates/wasm-validator/examples/review_fuzz_corpus.rs` converts an opaque libFuzzer corpus directory into a stable TSV inventory containing:

- a deterministic FNV-1a suggested ID;
- byte length;
- observed `valid` / `validation-error` / `parse-error` classification;
- the complete hex payload;
- original corpus filename;
- parser or validator diagnostic detail.

Run it locally with:

```text
cargo run -p wasm-validator --example review_fuzz_corpus -- fuzz/corpus/parse_validate
```

Promotion is deliberately review-gated rather than automatic:

1. inspect a PR-smoke or scheduled-campaign corpus-review TSV together with coverage/crash evidence;
2. choose a minimized input that represents a useful blind spot or reproducer;
3. add a named row to `fuzz/seeds/manifest.tsv` with the appropriate target set and expected stage;
4. run the deterministic seed replay plus the affected fuzz smoke;
5. when the input exposes an actual bug, also add a focused regression assertion for the exact invariant/error/behavior before or with the fix. The seed manifest is a stable trust-boundary replay corpus, not a substitute for a bug-specific oracle.

CI never edits the manifest or commits a corpus automatically. This keeps generated discovery material separate from reviewed regression evidence.

## Local use

```text
rustup toolchain install nightly --profile minimal --component llvm-tools-preview
cargo +nightly install cargo-fuzz --locked
python3 fuzz/materialize-seeds.py parse_module
python3 fuzz/materialize-seeds.py parse_validate
cargo +nightly fuzz run parse_module -- -dict=fuzz/wasm.dict
cargo +nightly fuzz run parse_validate -- -dict=fuzz/wasm.dict
```

For a bounded replay/smoke run, use fixed libFuzzer settings such as `-seed=1 -runs=512 -max_len=4096 -timeout=5 -rss_limit_mb=1024`.

To replay a target's current corpus with source coverage and render reviewable reports:

```text
cargo +nightly fuzz coverage parse_module
bash fuzz/render-coverage.sh parse_module
```

The renderer fails closed unless the nightly `llvm-tools-preview` component, merged `coverage.profdata`, and expected instrumented target binary are all present. It writes a human-readable summary, LCOV export, and browsable HTML report beneath `fuzz/coverage/<target>/report/`.

## PR smoke

`.github/workflows/fuzz-smoke.yml` materializes both reviewed seed sets, runs the deterministic manifest replay, performs bounded fuzzing for both targets, exercises the real source-coverage path, and emits `parse_module.tsv` plus `parse_validate.tsv` review inventories as a seven-day artifact. This validates the entire seed-to-fuzz-to-review loop without turning every pull request into a long campaign.

## Scheduled campaigns

`.github/workflows/fuzz-campaign.yml` runs both targets independently every Monday and can also be started manually. Each target receives a 15-minute coverage-guided libFuzzer campaign with a deterministic seed, the Wasm dictionary, a 4 KiB input limit, a five-second per-input timeout, and a 1 GiB RSS limit. `cargo-fuzz` uses its sanitizer-backed fuzz build, while the ordinary PR smoke remains short and path-filtered.

The workflow restores the latest target-specific corpus from the GitHub Actions cache, materializes any newly reviewed committed seeds, and replays the seed manifest before fuzzing. A successful campaign runs `cargo fuzz cmin`, uploads the minimized corpus snapshot for 14 days, replays that minimized corpus with `cargo fuzz coverage`, and uploads the merged profile plus summary/LCOV/HTML reports for 14 days. The coverage artifact also contains `corpus-review.tsv`, generated from the minimized corpus after coverage rendering so reviewers can inspect exact bytes and parser/validator classifications without unpacking opaque libFuzzer filenames manually. The minimized cache state is saved under a run-unique key so later campaigns continue from accumulated coverage without mutating the repository.

Crash handling is intentionally separate from reporting failures. Only a failed libFuzzer campaign triggers the target-specific crash-artifact upload; a later minimization, review, or coverage-report failure still fails the job but is not mislabeled as a discovered crash. Crash artifacts are retained for 30 days.

Cached/generated corpus and coverage data are discovery material, not regression oracles and not coverage gates. Coverage reports are evidence for finding blind spots, not a fabricated percentage threshold. A real reproducer still needs review, deterministic minimization where appropriate, promotion into the manifest, and a focused regression assertion when it represents a bug.

## Security boundary

The fuzz targets deliberately stop before instantiation/execution. Runtime fuzzing needs separate termination and resource controls because valid Wasm can loop, recurse, grow memory, and call hosts. Existing deterministic runtime adversarial tests continue to cover those boundaries.

Generated corpora, review inventories, coverage output, and crash artifacts are ignored local/CI working data. Only reviewed manifest seeds and focused regressions belong in source control.
