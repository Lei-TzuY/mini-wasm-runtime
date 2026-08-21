# Phase 6 — deterministic mutation robustness

This slice adds a deterministic malformed-input robustness layer around the public parser and validator. Its goal is narrow: ordinary byte corruption must resolve to normal parse/validation outcomes rather than Rust panics.

It is not a substitute for coverage-guided fuzzing. The normal CI suite needs stable, replayable mutations; fuzzing remains a separate Phase 6 surface with different tooling and acceptance criteria.

## Seed modules

The corpus starts from three valid raw WebAssembly modules selected to exercise different binary and validation paths:

- an arithmetic function with type, function, export, and code sections;
- linear memory with an active data segment and a memory load;
- a funcref table, immutable numeric global, function/table/global exports, an active element segment, and a function reading the global.

Every seed is first required to parse and validate successfully. This prevents a broken seed generator from making the malformed-input corpus vacuously pass.

## Deterministic mutations

Each seed is expanded through three mutation families.

### Exhaustive strict truncation

Every prefix shorter than the complete seed is sent through the parser. This systematically exercises EOF handling at section headers, LEB immediates, names, section payloads, function bodies, constant expressions, and segment encodings represented by the seeds.

### Exhaustive single-byte XOR mutations

Every byte position is independently XORed with four masks:

- `0x01`, a low-bit perturbation;
- `0x40`, a payload/sign-relevant bit for several encodings;
- `0x80`, the LEB continuation/high-bit perturbation;
- `0xff`, a maximal byte inversion.

This gives reproducible local corruption across header, section, type, name, immediate, opcode, and payload bytes.

### Fixed-seed multi-edit mutations

A committed xorshift64 seed generates 256 additional mutations per seed. Each mutation applies one to four byte edits at deterministic positions with deterministic non-zero XOR masks.

Together the three families produce well over one thousand mutated binaries on every normal test run.

## Panic contract

`parse_module` is executed under `catch_unwind`. A parser panic is not converted into an accepted error; the test immediately fails with the mutation label that triggered it.

If parsing succeeds, `wasm_validator::validate` is also executed under panic detection. A normal `ValidationError` is expected for many mutated modules, while a validator panic fails the test.

The mutation label records the seed plus truncation position, byte/mask, or generated case, making every failure directly replayable from the committed test.

## Why runtime execution is excluded

Arbitrary mutated code is deliberately not invoked in this slice. Even a structurally valid mutation can form long-running or intentionally trapping programs, so blindly executing every mutation would mix parser/validator robustness with runtime scheduling and resource-budget questions.

Runtime malformed-state and adversarial-execution coverage remains a separate hardening target.

## Limits of `catch_unwind`

This corpus detects Rust panics that unwind. It does not guarantee recovery from process aborts, allocator termination, stack exhaustion, or operating-system kills. PR #20 separately removed the known parser allocation-amplification path caused by preallocating from untrusted vector counts, but that does not turn this corpus into a general resource-exhaustion proof.

## Relationship to fuzzing

The corpus is deterministic mutation testing, not fuzzing. It has no coverage feedback, corpus evolution, minimization/shrinking, sanitizer integration, or long-running exploration. Those capabilities can discover input shapes this committed mutation set does not reach.

Accordingly, the roadmap keeps parser fuzzing open even though this deterministic regression layer is complete.
