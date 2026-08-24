# Phase 6 deterministic benchmark tranche

The repository includes an isolated `benchmarks/` workspace for fixed interpreter workloads. Benchmark tooling stays outside the product workspace so it does not change production dependencies or the Rust 1.81 MSRV.

The workload set exercises integer arithmetic/loops, `br_table` control dispatch, typed i64 memory store/load behavior, and f64 sign-bit operations. Modules are fixed WAT inputs compiled at benchmark startup, then parsed and instantiated once before warmup and measurement.

Every invocation is semantically checked against an exact expected result and folded into a deterministic checksum. Timing begins only after module creation and warmup, so the reported execution metric measures repeated exported-function invocation rather than WAT compilation, parsing, validation, or instantiation.

The harness now supports repeated timing samples and reports median nanoseconds per iteration plus median absolute deviation (MAD). Normal smoke runs may use one sample, while controlled baseline creation/checking requires at least seven samples and an explicit host ID.

Controlled baselines are fail-closed. A baseline records its host ID, iteration/warmup/sample settings, policy constants, workload names, deterministic workload-definition fingerprints, and per-workload median/MAD. Each fingerprint covers the benchmark name, WAT definition, result type, and expected result bits. Checking requires the same host ID, identical measurement settings, the exact current workload set, matching fingerprints, and the same compiled policy constants. Baseline files with schema drift, duplicate/stale/missing workloads, changed workload definitions, malformed values, or policy drift are rejected.

Both baseline and candidate measurements must have relative MAD at or below 10%. For each workload, a regression is reported only when:

```text
candidate_median > baseline_median * 1.10 + 3 * max(baseline_MAD, candidate_MAD)
```

This policy requires both a 10% practical slowdown and separation beyond a three-MAD noise allowance. It is intentionally conservative: an unstable run is rejected as untrustworthy instead of being called either a pass or a regression.

Baseline creation is also deliberate. `--write-baseline` uses create-new semantics and refuses to overwrite an existing file; refreshing a baseline therefore requires an explicit removal/new path plus normal code review. Changing a workload invalidates its fingerprint and requires a new reviewed baseline instead of comparing unlike code. `benchmarks/baselines/` is reserved for reviewed measurements from identified controlled hosts.

The dedicated GitHub Actions benchmark smoke continues to run release-mode workloads with bounded iteration counts, plus formatting, Clippy, and unit tests for statistics/baseline parsing/policy/fingerprints. Shared GitHub-hosted runner timings are never fed into `--check-baseline`, so variable hosted hardware cannot create performance pass/fail decisions.

The first trusted machine-specific baseline is intentionally not fabricated from CI. Recording one on a pinned host, with its machine configuration documented in the review, remains the final operational step for continuous performance gating.
