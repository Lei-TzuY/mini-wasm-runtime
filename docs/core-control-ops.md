# Core control and parametric instructions

This slice adds four previously fail-closed MVP instructions: `nop`, `drop`, untyped `select` (`0x1b`), and `br_table`.

## Validation invariants

- `nop` has no stack effect.
- `drop` consumes one value of any currently supported numeric type and respects unreachable-stack polymorphism.
- `select` consumes `val1 val2 i32` and produces one value; the two candidate values must have the same type.
- `br_table` validates every table target plus the default target, requires identical label type vectors, consumes an i32 selector, validates the shared label-result vector, and makes following code unreachable.
- typed select (`0x1c`) remains unsupported.

## Runtime invariants

- every instruction remains fuel-metered, including `nop`.
- `drop` retains a runtime stack-underflow guard as defense in depth.
- `select` checks candidate value variants again before choosing the first value for nonzero or the second for zero.
- `br_table` treats the selector as unsigned i32 bits, chooses an indexed target when in range and otherwise the default, then reuses the existing depth-based branch unwinding path.
- `br_table` decoding streams depths and does not allocate from its target count.

## Coverage

The focused integration corpus covers `nop`/`drop` execution, both `select` branches, mismatched `select` operand types, indexed/default `br_table` dispatch, and mixed-label-signature rejection. Existing fail-closed opcode checks now use typed select (`0x1c`) so newly supported `nop` is not mistaken for an unsupported instruction.

Unsupported proposal instructions remain fail-closed.
