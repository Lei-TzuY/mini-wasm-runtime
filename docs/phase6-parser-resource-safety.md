# Phase 6 parser resource safety

The binary parser treats all vector counts as attacker-controlled input. Resource use must therefore follow successfully decoded bytes rather than declared counts that have not yet been substantiated by the payload.

## Allocation invariant

A decoded u32 vector count must not, by itself, cause capacity proportional to that count to be allocated before entries are decoded.

Before this hardening slice, multiple parser paths immediately passed binary counts to `Vec::reserve` or `Vec::with_capacity`, including section entry vectors, function type values, element function indices, and code local groups. A tiny malformed payload could declare `u32::MAX` and provoke an enormous allocation attempt before the next read discovered EOF.

The parser now grows these vectors incrementally. The semantic loops still honor the encoded count exactly; the only change is when capacity is acquired. For a valid module, entries are decoded and pushed exactly as before. For a truncated module with an absurd count, parsing reaches the first missing entry and fails with the existing decode error without first attempting capacity based on the attacker-controlled declaration.

## Regression corpus

`allocation_bomb_corpus.rs` uses tiny raw binaries with `u32::MAX` counts for representative nested and top-level vectors:

- function type parameter count
- import entry count
- element function-index count
- code local-group count
- data segment count

Each fixture must fail as `UnexpectedEof` while decoding absent entries. The test intentionally avoids measuring allocator behavior directly; the production invariant is enforced structurally by removing count-driven preallocation, while the fixtures ensure absurd counts still fail quickly through ordinary parsing.

## Tradeoff

Removing attacker-controlled upfront reservation can cause normal `Vec` growth for large valid vectors. Rust's vector growth is amortized, and this is preferred to trusting an unvalidated declaration for a potentially enormous allocation. A future optimization may reserve only from a proven payload-derived bound, but must preserve the invariant above.

## Non-goals

This slice does not impose arbitrary module-size limits, replace the allocator, add parser fuzzing, or define the complete runtime threat model. Runtime fuel/memory limits and future parser-wide budgets remain separate hardening concerns.
