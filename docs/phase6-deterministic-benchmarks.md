# Phase 6 deterministic benchmark tranche

The repository now includes an isolated `benchmarks/` workspace for fixed interpreter workloads. Benchmark tooling stays outside the product workspace so it does not change production dependencies or the Rust 1.81 MSRV.

The first workload set exercises integer arithmetic/loops, `br_table` control dispatch, typed i64 memory store/load behavior, and f64 sign-bit operations. Modules are fixed WAT inputs compiled at benchmark startup, then parsed and instantiated once before warmup and measurement.

Every invocation is semantically checked against an exact expected result and folded into a deterministic checksum. Timing begins only after module creation and warmup, so the reported execution metric measures repeated exported-function invocation rather than WAT compilation, parsing, validation, or instantiation.

The dedicated CI smoke runs release-mode workloads with bounded iteration counts. Shared GitHub-hosted runner timings are recorded but never used as pass/fail performance thresholds; controlled-host baselines and a statistically justified regression policy remain follow-up work.
