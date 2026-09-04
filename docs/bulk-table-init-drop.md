# Bulk table: `table.init` and `elem.drop`

This vertical slice adds executable bulk-memory `table.init` (`0xfc 12`) and `elem.drop` (`0xfc 13`) for the existing single funcref-table surface. Passive element payloads are stored per instance; active and declarative segments begin unavailable to bulk initialization.

Validation checks element/table indices and the three i32 operands. Execution uses unsigned table32 addressing, preflights both source and destination ranges before mutation, updates imported `TableHandle` backing directly, and makes dropped segments empty for subsequent initialization.
