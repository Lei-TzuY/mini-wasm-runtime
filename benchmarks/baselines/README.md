# Controlled benchmark baselines

This directory is reserved for reviewed performance baselines captured on explicitly identified, pinned hosts. Do not populate it with GitHub-hosted runner timing.

A baseline is created with `--write-baseline` and contains the host ID, exact iteration/warmup/sample settings, fixed regression-policy constants, and per-workload median/MAD timing. The benchmark binary refuses to overwrite an existing baseline, and `--check-baseline` rejects host, settings, workload-set, schema, or policy mismatches.

Before committing a new or refreshed baseline, record the machine configuration in the change description and verify stable power/thermal/background-load conditions. A baseline refresh should be reviewed as a performance decision, not used to make an unexpected regression disappear.
