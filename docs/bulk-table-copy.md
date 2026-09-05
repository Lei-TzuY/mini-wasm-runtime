# Bulk table: `table.copy`

Adds executable `table.copy` (`0xfc 14`) for the current single `funcref` table surface. Validation requires three i32 operands and fails closed on non-zero table indices. Runtime preflights source and destination ranges before mutation and snapshots the source range, preserving overlap-safe memmove semantics while mutating imported `TableHandle` backing directly.
