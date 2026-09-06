# Bulk table: `table.grow`

This slice adds executable `table.grow` (`0xfc 15`) for the current single-`funcref` table surface. Typed validation consumes a `funcref` initializer and an `i32` delta and returns the previous table size as `i32`.

Runtime growth applies to both module-owned and imported/shared `TableHandle` values. New slots are initialized with either null or an instance-owned non-null function reference. A zero delta returns the current size without mutation. Growth beyond the declared maximum, integer overflow, or allocation failure returns `-1` and leaves the table unchanged.

The slice intentionally preserves the existing one-table, `funcref`-only boundary. Multi-table, `externref`, memory64, and reference-typed host ABI remain separate capabilities.
