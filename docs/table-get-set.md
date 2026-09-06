# Table reference access: `table.get` / `table.set`

This slice adds executable core `table.get` (`0x25`) and `table.set` (`0x26`) for the current single-`funcref` table surface. Typed validation requires an `i32` element index and preserves `funcref` values; runtime reads and writes nullable and non-null references without encoding them as integers.

Reads validate that a stored function reference belongs to the current instance before materializing `Value::FuncRef`, and writes re-materialize non-null references with the current instance identity. The same path applies to module-owned and imported/shared `TableHandle` values. Out-of-bounds element accesses trap before mutation, unsupported non-zero table indices fail closed, and numeric operands cannot be substituted for references.

The slice intentionally keeps the existing one-table, `funcref`-only boundary. `table.grow`, multi-table, `externref`, reference-typed host ABI, and cross-instance reference transfer remain separate capabilities.
