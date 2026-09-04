# Bulk memory: `memory.fill`

This vertical slice adds executable bulk-memory `memory.fill` (`0xfc 11`) for the existing single memory32 surface. Validation consumes the single memory index, requires `(destination, value, length)` as i32 operands, and keeps non-zero memory indices fail-closed until multi-memory is implemented.

Execution interprets destination and length as unsigned memory32 values, uses the low eight bits of the fill value, and preflights the complete destination range before mutation. Defined memories and imported `MemoryHandle` backing use the same runtime path.

Focused regressions cover low-byte semantics, imported-memory visibility, out-of-bounds atomicity, and rejection of non-zero memory indices.
