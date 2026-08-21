from pathlib import Path

p = Path("crates/wasm-runtime/src/numeric.rs")
text = p.read_text()
old = '''pub(super) fn binary_i32(
    stack: &mut Vec<Value>,
    operation: fn(i32, i32) -> i32,
) -> Result<(), RuntimeError> {
    let rhs = i32_from_stack(stack)?;
    let lhs = i32_from_stack(stack)?;
    stack.push(Value::I32(operation(lhs, rhs)));
    Ok(())
}

pub(super) fn binary_i64(
    stack: &mut Vec<Value>,
    operation: fn(i64, i64) -> i64,
) -> Result<(), RuntimeError> {
    let rhs = match pop_typed(stack, ValueType::I64)? {
        Value::I64(value) => value,
        _ => unreachable!("pop_typed established i64"),
    };
    let lhs = match pop_typed(stack, ValueType::I64)? {
        Value::I64(value) => value,
        _ => unreachable!("pop_typed established i64"),
    };
    stack.push(Value::I64(operation(lhs, rhs)));
    Ok(())
}

'''
if text.count(old) != 1:
    raise SystemExit("superseded integer helper anchor mismatch")
p.write_text(text.replace(old, "", 1))
