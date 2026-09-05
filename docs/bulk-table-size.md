# Bulk table: `table.size`

Adds executable `table.size` (`0xfc 16`) for the current single `funcref` table surface. Validation accepts table index 0 only and pushes an `i32`; runtime reports the live `TableHandle` length, including imported tables, and fails closed on unsupported non-zero table indices.

`table.grow` and `table.fill` remain out of scope for this slice because the operand stack does not yet carry reference values; they should land with an explicit reference-value model rather than a numeric-stack shortcut.
