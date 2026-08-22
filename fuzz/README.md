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

## Security boundary

The fuzz targets deliberately stop before instantiation/execution. Runtime fuzzing needs separate termination and resource controls because valid Wasm can loop, recurse, grow memory, and call hosts. Existing deterministic runtime adversarial tests continue to cover those boundaries.

Generated corpora and crash artifacts are local working data and are not committed by default. Reproducers that expose a real bug should be minimized and promoted into a deterministic regression corpus before the fix lands.
