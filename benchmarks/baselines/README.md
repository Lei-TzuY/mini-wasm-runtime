# Controlled benchmark baselines

This directory is reserved for reviewed performance baselines captured on explicitly identified, pinned hosts. Do not populate it with GitHub-hosted runner timing.

A baseline is created with `--write-baseline` and contains the host ID, exact iteration/warmup/sample settings, fixed regression-policy constants, and each workload's deterministic definition fingerprint plus median/MAD timing. The fingerprint covers the benchmark name, WAT, result type, and expected result bits. The benchmark binary refuses to overwrite an existing baseline, and `--check-baseline` rejects host, settings, workload-set, workload-definition, schema, or policy mismatches.

Before committing a new or refreshed baseline, record the machine configuration in the change description and verify stable power/thermal/background-load conditions. A benchmark-definition change intentionally invalidates the old fingerprint and requires a newly reviewed baseline. A baseline refresh should be reviewed as a performance decision, not used to make an unexpected regression disappear.
