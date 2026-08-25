# Parser fuzzing

This directory is an isolated `cargo-fuzz` workspace. It does not change the product workspace's Rust 1.81 MSRV or add a production runtime dependency.

## Targets

- `parse_module`: feeds arbitrary bytes directly to `wasm_parser::parse_module` and treats any panic/abort surfaced by libFuzzer as a bug.
- `parse_validate`: parses arbitrary bytes and, only when parsing succeeds, passes the resulting module into `wasm_validator::validate`. This stresses the parser-to-validator trust boundary without invoking arbitrary code.

## Local use

```text
rustup toolchain install nightly --profile minimal
cargo +nightly install cargo-fuzz --locked
cargo +nightly fuzz run parse_module -- -dict=fuzz/wasm.dict
cargo +nightly fuzz run parse_validate -- -dict=fuzz/wasm.dict
```

For a bounded replay/smoke run, use fixed libFuzzer settings such as `-seed=1 -runs=512 -max_len=4096 -timeout=5 -rss_limit_mb=1024`.

## Scheduled campaigns

`.github/workflows/fuzz-campaign.yml` runs both targets independently every Monday and can also be started manually. Each target receives a 15-minute coverage-guided libFuzzer campaign with a deterministic seed, the Wasm dictionary, a 4 KiB input limit, a five-second per-input timeout, and a 1 GiB RSS limit. `cargo-fuzz` uses its sanitizer-backed fuzz build, while the ordinary PR smoke remains short and path-filtered.

The workflow restores the latest target-specific corpus from the GitHub Actions cache before fuzzing. A successful campaign runs `cargo fuzz cmin` and then uploads the minimized corpus snapshot for 14 days; the minimized cache state is also saved under a run-unique key so future scheduled campaigns can continue from accumulated coverage without mutating the repository. If libFuzzer finds a crash, the target job fails and uploads the target's crash artifacts for 30 days instead of pretending the corpus is clean.

Cached/generated corpus data is discovery material, not a regression oracle. A real reproducer still needs review, deterministic minimization where appropriate, and promotion into the repository's normal regression corpus before a bug fix lands. The scheduled workflow does not auto-commit fuzz output.

## Security boundary

The fuzz targets deliberately stop before instantiation/execution. Runtime fuzzing needs separate termination and resource controls because valid Wasm can loop, recurse, grow memory, and call hosts. Existing deterministic runtime adversarial tests continue to cover those boundaries.

Generated corpora and crash artifacts are local working data and are not committed by default. Reproducers that expose a real bug should be minimized and promoted into a deterministic regression corpus before the fix lands.
