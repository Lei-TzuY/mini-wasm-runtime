# Imported-global differential regression fixtures

This directory is the reviewed replay destination for minimized differential captures that depend on a host-owned mutable i32 global. The WAT module alone is not enough to reproduce these cases because the host deliberately overrides the imported backing between guest calls.

`manifest.tsv` has exactly nine tab-separated fields per non-comment row:

```text
id	fixture	behavior	initial_state	override_call	override_value	inputs	expected_results	expected_final_state
```

The initial supported behavior is `mutable_i32_global`, matching the driver emitted by `imported_global_mismatch_capture.rs`. The host creates one mutable i32 global, initializes it from `initial_state`, replaces it with `override_value` immediately before the zero-based `override_call`, and then invokes the guest once per comma-separated input. The guest wrapping-adds each input into the imported global and returns the new value.

The replay harness rejects duplicate IDs or fixture paths, unsafe/non-WAT paths, missing files, malformed rows, unsupported behaviors, empty traces, invalid i32/usize fields, out-of-range override calls, result/input length mismatches, and manifest expectations that disagree with the independent recurrence model. It compiles each WAT once and requires both the mini runtime and Wasmtime to match the complete declared result trace and final host-visible global value.

The seeded fixtures establish ordinary and wrapping host-override behavior; they are regression guards, not claims of previously observed production bugs.

When CI emits an `auto-import-global-*.wat` plus companion `.global.tsv`, review the minimized behavior and provenance first. Promotion is deliberate: copy the `.wat` here, append the driver row to `manifest.tsv`, keep the generated ID stable, and rerun the full differential suite. CI never edits this committed corpus automatically.
