# Phase 5C pinned WAST address/alignment tranche

This tranche extends the manifest-driven upstream WAST corpus without changing runtime behavior.

## Provenance

- upstream repository: `WebAssembly/spec`
- pinned commit: `fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- selected sources:
  - `test/core/address.wast`
  - `test/core/align.wast`

The committed fixtures retain the pinned commit and source path in-file so provenance drift is caught by the existing manifest guard.

## Selected scope

`phase5c_upstream_address_subset.wast` contains one source-faithful module and 61 executable assertions. It covers:

- i32 8-bit, 16-bit, and 32-bit loads with explicit offsets and alignment hints
- normal data reads at address zero
- legal reads at the tail of a 64 KiB page
- a load whose effective range crosses the page boundary
- negative i32 addresses interpreted through the 32-bit address space
- maximum `u32` offsets that must trap rather than wrap into the allocation

`phase5c_upstream_align_subset.wast` contains 23 source-faithful modules and no execution assertions. The modules exercise every currently supported numeric load/store family with its legal natural alignment annotation, including i32, i64, f32, and f64 forms.

## Exact accounting

| source | modules | executed assertions | filtered directives |
| --- | ---: | ---: | ---: |
| `test/core/address.wast` | 1 | 61 | 0 |
| `test/core/align.wast` | 23 | 0 | 0 |

After this tranche the committed manifest covers 23 unique upstream sources and 640 selected assertions, still with zero filters.

## Explicit exclusions

This tranche does not silently ingest unsupported directives. It intentionally leaves out:

- the remaining `address.wast` sections not selected for this focused boundary tranche
- `align.wast` malformed-text assertions
- `align.wast` invalid modules whose alignment exceeds the natural width
- any proposal-dependent memory forms outside the runtime's current single 32-bit memory surface

Those cases are omitted from the curated fixture rather than counted as filtered directives. The broader roadmap item for remaining supported WAST coverage therefore stays open.

## Validation contract

The existing WAST runner must report the exact manifest counts above. Any unexpected module failure, assertion mismatch, trap mismatch, duplicate source/fixture, provenance drift, or newly filtered directive fails the test.

The focused gate is:

```bash
cargo test -p wasm-runtime --test phase5c_wast_ingestion
```

The tranche is additionally covered by workspace formatting, Clippy, tests, docs, benchmark smoke, and Wasmtime differential smoke in pull-request CI.
