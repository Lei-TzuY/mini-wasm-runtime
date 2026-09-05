from pathlib import Path


def replace(path, old, new):
    p = Path(path)
    text = p.read_text()
    if old not in text:
        raise SystemExit(f"anchor not found in {path}: {old[:80]!r}")
    p.write_text(text.replace(old, new, 1))

runtime = "crates/wasm-runtime/src/lib.rs"
replace(runtime,
'''                0xd1 => {
                    let value = stack.pop().ok_or(RuntimeError::StackUnderflow)?;
                    let reference = match value {
                        Value::FuncRef(reference) => reference,
                        other => {
                            return Err(RuntimeError::ValueTypeMismatch {
                                expected: ValueType::FuncRef,
                                actual: other.value_type(),
                            })
                        }
                    };
                    stack.push(Value::I32(if reference.is_none() { 1 } else { 0 }));
                }
                0xfc => {''',
'''                0xd1 => {
                    let value = stack.pop().ok_or(RuntimeError::StackUnderflow)?;
                    let reference = match value {
                        Value::FuncRef(reference) => reference,
                        other => {
                            return Err(RuntimeError::ValueTypeMismatch {
                                expected: ValueType::FuncRef,
                                actual: other.value_type(),
                            })
                        }
                    };
                    stack.push(Value::I32(if reference.is_none() { 1 } else { 0 }));
                }
                0xd2 => {
                    let function_index = read_u32_immediate(code, &mut pc)?;
                    stack.push(Value::FuncRef(Some(function_index)));
                }
                0xfc => {''')
replace(runtime,
'''            0xd1 => {}
            0xfc => {''',
'''            0xd1 => {}
            0xd2 => {
                let _ = read_u32_immediate(code, &mut pc)?;
            }
            0xfc => {''')

validator = "crates/wasm-validator/src/typed.rs"
replace(validator,
'''            0xd1 => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::FuncRef,
                    ValueType::I32,
                    function,
                    offset,
                )?;
            }
            0xfc => {''',
'''            0xd1 => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::FuncRef,
                    ValueType::I32,
                    function,
                    offset,
                )?;
            }
            0xd2 => {
                let target = read_u32(code, &mut pc, function, offset)?;
                if function_type(module, target).is_none() {
                    return Err(ValidationError::CallTargetOutOfBounds {
                        function,
                        offset,
                        target,
                    });
                }
                stack.push(ValueType::FuncRef);
            }
            0xfc => {''')

test = Path("crates/wasm-runtime/tests/reference_null.rs")
text = test.read_text()
text += '''
#[test]
fn ref_func_is_non_null_and_executes() {
    let module = parse_module(&module(&[0xd2, 0x00, 0xd1])).unwrap();
    let mut instance = Instance::new(module).unwrap();
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(0))
    );
}

#[test]
fn ref_func_can_be_dropped() {
    let module = parse_module(&module(&[0xd2, 0x00, 0x1a, 0x41, 0x07])).unwrap();
    let mut instance = Instance::new(module).unwrap();
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(7))
    );
}

#[test]
fn ref_func_rejects_out_of_bounds_function_index() {
    let module = parse_module(&module(&[0xd2, 0x01, 0xd1])).unwrap();
    assert!(matches!(
        Instance::new(module),
        Err(RuntimeError::Validation(ValidationError::CallTargetOutOfBounds {
            target: 1,
            ..
        }))
    ));
}
'''
test.write_text(text)

doc = Path("docs/reference-null.md")
doc.write_text(doc.read_text() + '''\n\n## Non-null function references\n\n`ref.func` (`0xd2`) now materializes a validated non-null `funcref` operand for an existing function index. `ref.is_null` therefore distinguishes `ref.func` from `ref.null funcref`. This slice intentionally does not broaden function signatures, locals/globals, or the host ABI to reference values.\n''')
