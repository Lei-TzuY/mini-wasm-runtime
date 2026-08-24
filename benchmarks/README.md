# Deterministic execution benchmarks

This nested workspace provides fixed mini-runtime workloads without adding benchmark dependencies to the product workspace.

The initial workloads cover:

- integer arithmetic and a bounded loop
- structured control with `br_table`
- repeated i64 memory store/load operations
- f64 sign-bit operations followed by reinterpretation

Each workload compiles a fixed WAT module, parses and instantiates it once, performs a configurable warmup, then times repeated `run` export invocations. Every invocation must return an exact expected value, and measured results are folded into a deterministic checksum so semantic drift fails before timing data is trusted.

A normal smoke run remains simple:

```bash
cargo run --release --manifest-path benchmarks/Cargo.toml -- --iterations 1000 --warmup 64
```

`--samples N` repeats each timed workload and reports the median, median absolute deviation (MAD), minimum, maximum, deterministic checksum, and a deterministic workload fingerprint. The fingerprint binds the benchmark name, WAT definition, result type, and expected result bits so an old timing baseline cannot silently be reused after the workload itself changes. One sample is enough for CI smoke; controlled comparisons require at least seven samples.

## Controlled-host baselines

Performance pass/fail is deliberately opt-in and bound to an explicit host identity. Capture a baseline only on a pinned machine with stable power, thermal, background-load, OS, compiler, and CPU-frequency conditions:

```bash
cargo run --release --manifest-path benchmarks/Cargo.toml -- \
  --iterations 2000 --warmup 128 --samples 9 \
  --host-id lab-box-a \
  --write-baseline benchmarks/baselines/lab-box-a.tsv
```

The writer refuses to overwrite an existing file. Review and version a trusted baseline deliberately rather than silently refreshing it after a slowdown.

Check a later candidate using the exact same host ID and measurement settings:

```bash
cargo run --release --manifest-path benchmarks/Cargo.toml -- \
  --iterations 2000 --warmup 128 --samples 9 \
  --host-id lab-box-a \
  --check-baseline benchmarks/baselines/lab-box-a.tsv
```

The baseline parser fails closed on schema/policy drift, host mismatch, measurement-setting mismatch, duplicate or stale workloads, missing workloads, malformed values, workload-fingerprint drift, or an unstable baseline. Candidate measurements also fail as inconclusive when relative MAD exceeds 10%. Changing a benchmark's WAT or expected result therefore requires a newly reviewed baseline instead of comparing unlike workloads.

For each workload, the regression limit is:

```text
baseline_median * 1.10 + 3 * max(baseline_MAD, candidate_MAD)
```

A candidate median strictly above that limit is a regression. This combines a 10% practical-effect margin with a three-MAD noise allowance instead of treating a single wall-clock sample as authoritative. Policy constants are encoded in both the benchmark binary and baseline file; a file with different constants is rejected rather than silently weakening the check.

## Hosted CI boundary

GitHub-hosted runner timing is noisy and hardware is not pinned. CI therefore checks benchmark formatting, Clippy, unit tests for baseline parsing/statistics/policy/fingerprints, successful release execution, exact workload results, and checksums. It never supplies `--check-baseline`, so hosted-runner wall-clock data cannot fail a pull request as a performance regression.

`benchmarks/baselines/` is reserved for reviewed measurements from identified controlled hosts. The repository does not fabricate a baseline from shared CI timing; the first trusted host measurement remains a deliberate follow-up.
