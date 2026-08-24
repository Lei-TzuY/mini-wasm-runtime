# Imported-table differential regression fixtures

This directory is the reviewed replay destination for minimized differential captures that require a host-owned funcref table. It is separate from both the no-import regression corpus and the other import-driver schemas because table mutations need an explicit host action and a per-call trap/result trace.

`manifest.tsv` has exactly eleven tab-separated fields per non-comment row:

```text
id	fixture	behavior	mutation_call	mutation	addend	xor_mask	values	selectors	expected_outcomes	final_slot_one
```

The initial supported behavior is `mutable_funcref_table`. Every fixture imports a two-slot funcref table, initializes slot 0 with an i32 wrapping-add target and slot 1 with an i32 xor target, then dispatches through `call_indirect`. Immediately before `mutation_call`, the host either copies slot 0 into slot 1 (`copy0to1`) or clears slot 1 (`clear1`).

`expected_outcomes` is a comma-separated trace using `i32:<value>`, `null`, or `oob`. The replay harness rejects malformed rows, duplicate IDs/fixtures, unsafe/non-WAT paths, missing files, unsupported behavior/mutation names, invalid numeric fields, empty or length-mismatched traces, out-of-range mutation points, unknown outcome tokens, invalid final-slot states, and expectations that disagree with the independent table-state model. It then compiles each WAT once and requires the mini runtime and Wasmtime to match the complete trace plus final host-visible slot-1 presence.

The seeded fixtures exercise host relocation, host clearing, null indirect-call traps, table-OOB traps, post-trap recovery on the same live instance, and ordinary target results. They establish the replay format and are not claims of previously observed production bugs.

When CI emits an `auto-import-table-*.wat` plus companion `.table.tsv`, review the minimized behavior and provenance first. Promotion is deliberate: copy the WAT here, append the driver row to `manifest.tsv`, keep the generated ID stable, and rerun the full differential suite. CI never edits this committed corpus automatically.
