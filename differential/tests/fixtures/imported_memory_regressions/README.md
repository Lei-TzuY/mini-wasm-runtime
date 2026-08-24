# Imported-memory differential regression fixtures

This directory is the reviewed replay destination for minimized differential captures that depend on host-owned imported linear memory. The WAT module alone is insufficient because the host seeds and deliberately overrides the imported backing between guest calls.

`manifest.tsv` has exactly ten tab-separated fields per non-comment row:

```text
id	fixture	behavior	address	initial_value	override_call	override_value	inputs	expected_results	expected_final_value
```

The initial supported behavior is `mutable_i32_memory`, matching the driver emitted by `imported_memory_mismatch_capture.rs`. The host creates a one-page memory with maximum two pages, writes `initial_value` at `address`, replaces those four bytes with `override_value` immediately before the zero-based `override_call`, and invokes the guest once per comma-separated input. The guest wrapping-adds each input to the i32 stored at that address, writes the result back, and returns it.

The replay harness rejects duplicate IDs or fixture paths, unsafe/non-WAT paths, missing files, malformed rows, unsupported behaviors, empty traces, invalid u32/i32/usize fields, addresses that cannot fit one i32 in the initial page, out-of-range override calls, result/input length mismatches, and manifest expectations that disagree with the independent recurrence model. It compiles each WAT once and requires both the mini runtime and Wasmtime to match the complete declared result trace and final host-visible memory value.

The seeded fixtures establish ordinary and wrapping updates, including the final valid four-byte address of a one-page memory; they are regression guards, not claims of previously observed production bugs.

When CI emits an `auto-import-memory-*.wat` plus companion `.memory.tsv`, review the minimized behavior and provenance first. Promotion is deliberate: copy the `.wat` here, append the driver row to `manifest.tsv`, keep the generated ID stable, and rerun the full differential suite. CI never edits this committed corpus automatically.
