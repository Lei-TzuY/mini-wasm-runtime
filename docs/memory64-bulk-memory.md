# Bounded memory64 bulk-memory semantics

This slice extends the existing bounded memory64 address-width model through the executable bulk-memory instructions without changing physical allocation ceilings or the host-memory ABI.

## Address typing

- `memory.init`: the destination operand follows the selected memory index type (`i32` for memory32, `i64` for memory64); the passive-data source offset and length remain `i32`.
- `memory.fill`: destination and length follow the selected memory index type; the fill value remains `i32`.
- `memory.copy`: destination and source each follow their selected memory index type. Length is `i64` only when both selected memories are memory64; mixed memory32/memory64 copies retain an `i32` length.

## Runtime boundary

The runtime keeps the existing bounded physical memory backing. Full-width memory64 operands are preserved through validation and runtime preflight; an address outside the available backing traps instead of being truncated into the low 32-bit address space. Source and destination bounds are checked before mutation so mixed-memory copies remain fail-closed.

## Evidence

Deterministic validator/runtime regressions cover memory64 `memory.init`, `memory.fill`, `memory.copy`, mixed memory32/memory64 copy typing, and an address at `2^32` that must reach runtime bounds checking without truncation. The isolated differential workspace uses pinned Wasmtime 37.0.3 with a single-page memory64 fixture to compare `memory.fill` plus `memory.copy` results and to verify that the same `2^32` fill destination traps in both engines without requiring a giant host allocation.

## Non-goals

This slice does not add imported memory64 host backing, larger physical allocations, shared memories/threads, or claim complete memory64 proposal conformance. Those remain independent milestones.
