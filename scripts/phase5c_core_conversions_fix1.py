from pathlib import Path

path = Path("scripts/phase5c_core_conversions.py")
text = path.read_text()

old_anchor = """    '''    stack.push(value);\n    Ok(())\n}\n''',"""
new_anchor = """    '''        0xbf => Value::F64(f64::from_bits(i64_from_stack(stack)? as u64)),\n        _ => return Err(RuntimeError::UnsupportedOpcode(opcode)),\n    };\n    stack.push(value);\n    Ok(())\n}\n''',"""
if text.count(old_anchor) != 1:
    raise SystemExit(f"expected one broad helper anchor, found {text.count(old_anchor)}")
text = text.replace(old_anchor, new_anchor, 1)

old_replacement_start = """    '''    stack.push(value);\n    Ok(())\n}\n\nfn trunc_to_i32"""
new_replacement_start = """    '''        0xbf => Value::F64(f64::from_bits(i64_from_stack(stack)? as u64)),\n        _ => return Err(RuntimeError::UnsupportedOpcode(opcode)),\n    };\n    stack.push(value);\n    Ok(())\n}\n\nfn trunc_to_i32"""
if text.count(old_replacement_start) != 1:
    raise SystemExit(
        f"expected one helper replacement start, found {text.count(old_replacement_start)}"
    )
text = text.replace(old_replacement_start, new_replacement_start, 1)
path.write_text(text)
