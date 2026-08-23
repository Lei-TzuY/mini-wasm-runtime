# Import differential regression fixtures

This directory is the reviewed replay destination for minimized differential captures that require host imports. It is intentionally separate from the no-import `regressions/` corpus because a WAT module alone is insufficient to reproduce host-owned state and call-sequence behavior.

`manifest.tsv` has exactly eight tab-separated fields per non-comment row:

```text
id	fixture	behavior	initial_state	salt	inputs	expected_results	expected_final_state
```

The initial supported behavior is `stateful_i64_add`, matching the driver emitted by `import_mismatch_capture.rs`: the host keeps an i64 state, wrapping-adds every guest input, returns the new state, and the guest XORs that result with the fixture's embedded salt.

The replay harness rejects duplicate IDs or fixture paths, unsafe/non-WAT paths, missing files, malformed rows, unsupported host behaviors, empty traces, malformed i64 values, result/input length mismatches, and manifest expectations that disagree with the independent recurrence model. It then compiles each WAT once and requires both the mini runtime and Wasmtime to match the complete declared guest-result trace and final host state.

The two seeded fixtures establish the replay format and include ordinary and wrapping state transitions; they are regression guards, not claims of previously observed production bugs.

When CI emits an `auto-import-host-*.wat` plus companion `.import.tsv`, review the minimized behavior and provenance first. Promotion is deliberate: copy the `.wat` here, append the driver row to `manifest.tsv`, keep the generated ID stable, and rerun the full differential suite. CI never edits this committed corpus automatically.
