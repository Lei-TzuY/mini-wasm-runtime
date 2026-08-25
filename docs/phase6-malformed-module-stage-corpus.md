# Phase 6 malformed-module stage corpus

The runtime test suite includes a focused adversarial corpus that separates parser-accepted modules rejected by validation, statically valid modules rejected during instantiation, and modules that are valid through instantiation but fail only during execution.

The corpus is intentionally stage-sensitive: later-stage cases must first survive every earlier trust boundary. Several cases start from a parser-accepted seed and then mutate the parsed module model directly so one invariant can be isolated without relying on a second malformed binary encoding.

## Validation-stage corpus

The original tranche covers function/code count mismatches, missing defined-function type targets, duplicate and out-of-range function exports, invalid local/call indices, memory instructions without memory, excessive memory alignment, invalid branch depth, malformed `else` structure, immutable global mutation, invalid start signatures, operand-stack underflow, typed operand mismatches, and invalid memory limits.

The expanded tranche adds exact rejection classes for:

- function-import type indices outside the type section
- out-of-range memory, table, and global exports
- table minimum/maximum inversions
- memories above the WebAssembly 65,536-page limit
- active element segments targeting a missing table
- element segments naming a missing function
- active data segments targeting a missing memory
- `call_indirect` table indices outside the table index space
- `call_indirect` type indices outside the type section
- start-function indices outside the combined function index space
- parsed-model mutations that remove the required final function `end`

These checks deliberately cross independent function/table/memory/global index spaces and segment descriptors rather than only exercising instruction-stack typing.

## Instantiation-stage corpus

A separate tranche proves that valid module structure is not enough to accept unresolved or incompatible host objects. Every case validates successfully before instantiation is attempted. The corpus now covers:

- unresolved function, global, table, and memory imports as distinct runtime errors
- exact host-function signature mismatch
- imported-global numeric type mismatch
- imported-global mutability mismatch
- imported-table limit mismatch under the runtime's WebAssembly subtyping rules
- imported-memory limit mismatch under the same fail-closed binding policy
- active data-segment bounds that are valid structurally but exceed the allocated linear memory
- active element-segment bounds that are valid structurally but exceed the allocated table

The active-segment cases are especially important because static validation checks index-space validity while concrete byte/slot bounds depend on instantiated object sizes.

## Execution-stage corpus

Execution cases must parse, validate, and instantiate before their dynamic failure is accepted. The initial tranche covers null and out-of-range `call_indirect` table elements, an out-of-bounds i32 store, and NaN-to-integer trapping conversion.

The expansion adds trap classes already normalized by the differential suite:

- signed integer division by zero
- signed integer overflow for `i32::MIN / -1`
- out-of-bounds i32 load at the end of a one-page memory
- `call_indirect` dynamic signature mismatch after a valid element segment installs a function of the wrong type

Keeping these in the stage corpus prevents a future validator from incorrectly rejecting dynamic-only behavior or a runtime refactor from silently converting precise traps into generic failures.

## Relationship to fuzzing and differential testing

The parser/validator fuzz pipeline now has a reviewed seed manifest and promotion flow. Those seeds stabilize parse/validation classifications, while this malformed-module suite gives focused semantic oracles for exact validator, instantiation, and execution errors. Differential replay provides the independent execution-side trap vocabulary; the stage corpus reuses representative normalized classes without adding Wasmtime to the product workspace.

This division is deliberate:

- fuzz seeds preserve discovery inputs and trust-boundary classification;
- malformed-module tests preserve exact layer ownership and error identity;
- differential tests preserve cross-engine observable semantics.

No generated corpus is auto-promoted into this file.

## Boundary

This tranche does not relax validation, add production dependencies, or change runtime behavior. It is deterministic test/documentation hardening only.

Future expansion should continue from reviewed fuzz discoveries, minimized differential regressions, new host/object surfaces, and segment/index-space edge cases. The roadmap item remains intentionally open-ended rather than claiming that an adversarial corpus can ever be complete.
