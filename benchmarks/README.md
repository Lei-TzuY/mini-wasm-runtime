# Deterministic execution benchmarks

This nested workspace provides fixed mini-runtime workloads without adding benchmark dependencies to the product workspace.

The initial workloads cover:

- integer arithmetic and a bounded loop
- structured control with `br_table`
- repeated i64 memory store/load operations
- f64 sign-bit operations followed by reinterpretation

Each workload compiles a fixed WAT module, parses and instantiates it once, performs a configurable warmup, then times repeated `run` export invocations. Every invocation must return an exact expected value, and measured results are folded into a deterministic checksum so semantic drift fails before timing data is trusted.

Run locally with:

```bash
cargo run --release --manifest-path benchmarks/Cargo.toml -- --iterations 1000 --warmup 64
```

Output is one line per workload with benchmark name, iteration/warmup counts, elapsed nanoseconds, nanoseconds per iteration, and checksum.

## What CI does not do

Hosted-runner timing is noisy. CI therefore checks formatting, Clippy, successful release execution, exact workload results, and checksums; it does not enforce wall-clock regression thresholds. A future controlled-host baseline can add statistically meaningful performance policy without turning shared-runner variance into false failures.
