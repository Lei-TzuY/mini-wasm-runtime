# Bulk memory: `memory.copy`

This vertical slice adds executable bulk-memory `memory.copy` (`0xfc 10`) for the runtime's existing single 32-bit memory surface. The validator consumes and validates both memory indices, requires the three i32 operands `(destination, source, length)`, and continues to reject non-zero memory indices until multi-memory is explicitly implemented.

Execution treats i32 operands as unsigned memory32 addresses/lengths, preflights both source and destination ranges before mutation, and uses memmove-equivalent overlap semantics. The operation is routed through the existing `with_memory_mut` abstraction, so defined memory and imported `MemoryHandle` backing share the same implementation.

The bounded regression suite covers overlapping copies, fail-closed out-of-bounds destination handling with no partial write, and non-zero memory-index rejection. Other bulk-memory instructions (`memory.init`, `data.drop`, table forms) remain out of scope for this slice.
