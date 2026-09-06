# Nullable `funcref` operands

This slice adds an executable nullable `funcref` operand model plus `ref.null funcref` (`0xd0 0x70`) and `ref.is_null` (`0xd1`). The validator tracks `funcref` as a real operand type, the interpreter carries nullable instance-local function indices, and structured-control predecode consumes the `ref.null` immediate. Invalid reference-type immediates and numeric operands fail closed.

The parser deliberately does not accept `funcref` in function signatures, locals, or globals yet. Reference-typed host parameters/results, reference globals, `ref.func`, `table.get`/`table.set`, `table.grow`, and `table.fill` remain unsupported until ownership and cross-instance semantics are implemented explicitly. This slice is the executable foundation for those capabilities; it does not encode references as integers.


## Non-null function references

`ref.func` (`0xd2`) now materializes a validated non-null `funcref` operand for an existing function index. `ref.is_null` therefore distinguishes `ref.func` from `ref.null funcref`. This slice intentionally does not broaden function signatures, locals/globals, or the host ABI to reference values.
