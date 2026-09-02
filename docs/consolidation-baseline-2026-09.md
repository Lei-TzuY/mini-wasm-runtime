# Phase 6 consolidation baseline — 2026-09

This document records the repository state immediately after PR #225 and defines the maintenance boundary for the next round of automated work.

## Frozen baseline

- baseline `main`: `e3b69156c146f8f168a80c72e44b089ec2f3d5bd`
- tree: `b997b0be055cfc63c43a7a8941f5fc1fbdefd224`
- pinned upstream spec: `WebAssembly/spec@fc209c5ed8afc4dfeb9252024d217da3376c7a6f`
- manifest: 33 unique upstream core WAST sources
- selected assertions: 1100
- stateful bare invokes: 4
- explicitly filtered directives: 0
- open pull requests at consolidation start: 0

The exact tree above was validated on the PR #225 head by the normal CI, Differential reference smoke, and Benchmark smoke workflows before squash merge.

This is a consolidation point, not a production-readiness claim. The interpreter remains intentionally incomplete and the existing fail-closed boundaries still apply.

## What is considered consolidated

The repository now has stable, explicit boundaries for:

- parser, validator, runtime, and host-boundary ownership;
- typed structured control and multi-value execution;
- i32/i64/f32/f64 numeric and memory operations implemented by the current MVP-oriented surface;
- imported/defined globals, tables, memory, and typed host callbacks;
- resource limits and runtime defense-in-depth checks;
- deterministic property/metamorphic and malformed-input regressions;
- Wasmtime differential execution and mismatch shrinking/replay;
- pinned WAST provenance with exact manifest accounting;
- fuzz smoke/campaign workflows and deterministic promotion of useful findings;
- benchmark smoke plus controlled-host baseline tooling.

Historical `phase5c-*` and `phase6-*` documents remain useful provenance, but they are not a reason to keep creating one document or one pull request per tiny conformance vector.

## Debt found during consolidation

### 1. Conformance work had become too fine-grained

Recent PRs were frequently source-faithful and correct but changed only a handful of already-supported arithmetic assertions. That improves coverage, but one PR per two-to-five vectors produces poor signal-to-noise once the instruction semantics are already established.

Future pure-conformance work should therefore be batched around a coherent upstream subsection or a meaningful boundary. A tiny assertion-only PR is justified only when it locks a high-risk trap/edge case or captures a real regression.

### 2. The WAST manifest documentation was stale

`docs/phase5c-pinned-wast-manifest.md` still described the original one-row tranche even though the executable manifest now covers 33 upstream sources. The manifest file and executable accounting remain authoritative; the documentation is being consolidated to describe the current contract rather than the historical first slice.

### 3. Branch hygiene needs periodic cleanup

The repository still contains many historical `feat/`, `test/`, `fuzz/`, `perf/`, and `automation/` branches from already-landed phases. They do not change `main`, but they make repository navigation noisier. Branch deletion is operational GitHub housekeeping and should be done periodically after confirming a branch has no unique unmerged work.

No history rewrite is required for this cleanup.

## Continuation gate for automated work

Automated continuation is allowed after this baseline, but a run should create a PR only when at least one of the following is true:

1. a minimal reproducer demonstrates a parser, validator, instantiation, runtime, or host-boundary correctness discrepancy;
2. a supported WebAssembly rule lacks a meaningful trap, malformed-input, or cross-layer regression;
3. a coherent pinned upstream WAST batch materially advances one source/subsection rather than adding a few routine vectors;
4. a differential or fuzz case exposes a mismatch, blind spot, or missing deterministic regression;
5. an existing hardening invariant can be strengthened without expanding a new subsystem;
6. documentation or manifest accounting is demonstrably inconsistent with executable state.

A run should make **no code change and open no PR** when the only available work is another tiny set of routine vectors with no distinct correctness value.

## Preferred work-unit size

For pure WAST expansion, prefer one coherent batch of roughly 20–50 assertions when the upstream source supports that size and the batch remains reviewable. Smaller batches remain appropriate for traps, malformed modules, stateful behavior, or cases that require production changes.

Production fixes stay regression-first and minimal. Conformance batching must never weaken validation, trap classification, fail-closed behavior, or exact manifest accounting.

## Rotation policy

Do not spend consecutive runs indefinitely on one numeric file. After one or two conformance batches, inspect other evidence surfaces before choosing the next task:

- parser / malformed binary handling;
- validator invariants;
- runtime traps and state transitions;
- memory/table/global/import aliasing;
- host capability boundaries;
- differential generation and normalization;
- fuzz promotion coverage;
- controlled benchmark methodology.

This rotation is a review discipline, not a requirement to invent work. A no-op run is preferable to artificial churn.

## Large features remain manual design decisions

The following are not suitable for an unattended bounded-maintenance task merely because the current backlog becomes quiet:

- WASI or other broad system interfaces;
- threads/shared memory;
- SIMD;
- memory64;
- multi-memory or multi-table proposal expansion;
- JIT compilation;
- major public host-ABI redesign.

Those require a separate design review and explicit scope before implementation.
