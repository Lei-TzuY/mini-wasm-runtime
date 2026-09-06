from pathlib import Path


def replace_once(path, old, new):
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected exactly one match in {path}, got {count}")
    p.write_text(text.replace(old, new, 1))


typed = "crates/wasm-validator/src/typed.rs"
replace_once(typed, '''            0x1b => {
                pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                let second = pop_any(&mut stack, &controls, function, offset)?;
                let first = pop_any(&mut stack, &controls, function, offset)?;
                let result_type = match (first, second) {
                    (Some(first), Some(second)) if first != second => {
                        return Err(ValidationError::TypeMismatch {
                            function,
                            offset,
                            expected: first,
                            actual: second,
                        });
                    }
                    (Some(first), Some(_)) | (Some(first), None) => Some(first),
                    (None, Some(second)) => Some(second),
                    (None, None) => None,
                };
                if let Some(result_type) = result_type {
                    stack.push(result_type);
                }
            }
            0x20 => {''', '''            0x1b => {
                pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                let second = pop_any(&mut stack, &controls, function, offset)?;
                let first = pop_any(&mut stack, &controls, function, offset)?;
                let result_type = match (first, second) {
                    (Some(first), Some(second)) if first != second => {
                        return Err(ValidationError::TypeMismatch {
                            function,
                            offset,
                            expected: first,
                            actual: second,
                        });
                    }
                    (Some(first), Some(_)) | (Some(first), None) => Some(first),
                    (None, Some(second)) => Some(second),
                    (None, None) => None,
                };
                if let Some(result_type) = result_type {
                    stack.push(result_type);
                }
            }
            0x1c => {
                let result_type = read_typed_select_type(code, &mut pc, function, offset)?;
                pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                pop_expect(&mut stack, &controls, result_type, function, offset)?;
                pop_expect(&mut stack, &controls, result_type, function, offset)?;
                stack.push(result_type);
            }
            0x20 => {''')
replace_once(typed, '''fn apply_call_signature(
''', '''fn read_typed_select_type(
    code: &[u8],
    pc: &mut usize,
    function: usize,
    offset: usize,
) -> Result<ValueType, ValidationError> {
    let count = read_u32(code, pc, function, offset)?;
    if count != 1 {
        return Err(ValidationError::MalformedImmediate { function, offset });
    }
    let tag = *code
        .get(*pc)
        .ok_or(ValidationError::MalformedImmediate { function, offset })?;
    *pc += 1;
    match tag {
        0x7f => Ok(ValueType::I32),
        0x7e => Ok(ValueType::I64),
        0x7d => Ok(ValueType::F32),
        0x7c => Ok(ValueType::F64),
        0x70 => Ok(ValueType::FuncRef),
        _ => Err(ValidationError::MalformedImmediate { function, offset }),
    }
}

fn apply_call_signature(
''')

runtime = "crates/wasm-runtime/src/lib.rs"
replace_once(runtime, '''                0x1b => {
                    let condition = numeric::i32_from_stack(&mut stack)?;
                    let second = stack.pop().ok_or(RuntimeError::StackUnderflow)?;
                    let first = stack.pop().ok_or(RuntimeError::StackUnderflow)?;
                    let expected = first.value_type();
                    let actual = second.value_type();
                    if actual != expected {
                        return Err(RuntimeError::ValueTypeMismatch { expected, actual });
                    }
                    stack.push(if condition != 0 { first } else { second });
                }
                0x20 => {''', '''                0x1b => {
                    let condition = numeric::i32_from_stack(&mut stack)?;
                    let second = stack.pop().ok_or(RuntimeError::StackUnderflow)?;
                    let first = stack.pop().ok_or(RuntimeError::StackUnderflow)?;
                    let expected = first.value_type();
                    let actual = second.value_type();
                    if actual != expected {
                        return Err(RuntimeError::ValueTypeMismatch { expected, actual });
                    }
                    stack.push(if condition != 0 { first } else { second });
                }
                0x1c => {
                    let expected = read_typed_select_type(code, &mut pc)?;
                    let condition = numeric::i32_from_stack(&mut stack)?;
                    let second = numeric::pop_typed(&mut stack, expected)?;
                    let first = numeric::pop_typed(&mut stack, expected)?;
                    stack.push(if condition != 0 { first } else { second });
                }
                0x20 => {''')
replace_once(runtime, '''            0x11 => {
                let _ = read_u32_immediate(code, &mut pc)?;
                let _ = read_u32_immediate(code, &mut pc)?;
            }
            0x28..=0x3e => {''', '''            0x11 => {
                let _ = read_u32_immediate(code, &mut pc)?;
                let _ = read_u32_immediate(code, &mut pc)?;
            }
            0x1c => {
                let _ = read_typed_select_type(code, &mut pc)?;
            }
            0x28..=0x3e => {''')
replace_once(runtime, '''fn build_control_map(module: &Module, code: &[u8]) -> Result<ControlMap, RuntimeError> {
''', '''fn read_typed_select_type(code: &[u8], pc: &mut usize) -> Result<ValueType, RuntimeError> {
    let count = read_u32_immediate(code, pc)?;
    if count != 1 {
        return Err(RuntimeError::ControlInvariant(
            "validated typed select must declare exactly one result type",
        ));
    }
    let tag = *code.get(*pc).ok_or(RuntimeError::ControlInvariant(
        "validated typed select result type is missing",
    ))?;
    *pc += 1;
    match tag {
        0x7f => Ok(ValueType::I32),
        0x7e => Ok(ValueType::I64),
        0x7d => Ok(ValueType::F32),
        0x7c => Ok(ValueType::F64),
        0x70 => Ok(ValueType::FuncRef),
        _ => Err(RuntimeError::ControlInvariant(
            "validated typed select result type is unsupported",
        )),
    }
}

fn build_control_map(module: &Module, code: &[u8]) -> Result<ControlMap, RuntimeError> {
''')

tests = r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::ValidationError;

fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    assert!(payload.len() < 128);
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

fn module(body_ops: &[u8]) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 0, 1, 0x7f]);
    section(&mut module, 3, &[1, 0]);
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);
    let mut body = vec![0];
    body.extend_from_slice(body_ops);
    body.push(0x0b);
    let mut code = vec![1, body.len() as u8];
    code.extend_from_slice(&body);
    section(&mut module, 10, &code);
    module
}

fn instantiate(body_ops: &[u8]) -> Result<Instance, RuntimeError> {
    Instance::new(parse_module(&module(body_ops)).expect("parse typed-select test module"))
}

#[test]
fn typed_select_i32_chooses_both_branches() {
    for (condition, expected) in [(1, 10), (0, 20)] {
        let mut vm = instantiate(&[
            0x41, 0x0a, 0x41, 0x14, 0x41, condition, 0x1c, 0x01, 0x7f,
        ])
        .unwrap();
        assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(expected)));
    }
}

#[test]
fn typed_select_funcref_preserves_nullability() {
    let mut non_null = instantiate(&[
        0xd2, 0x00, 0xd0, 0x70, 0x41, 0x01, 0x1c, 0x01, 0x70, 0xd1,
    ])
    .unwrap();
    assert_eq!(non_null.invoke_export("run", &[]).unwrap(), Some(Value::I32(0)));

    let mut null = instantiate(&[
        0xd2, 0x00, 0xd0, 0x70, 0x41, 0x00, 0x1c, 0x01, 0x70, 0xd1,
    ])
    .unwrap();
    assert_eq!(null.invoke_export("run", &[]).unwrap(), Some(Value::I32(1)));
}

#[test]
fn typed_select_rejects_operand_not_matching_declared_type() {
    let error = instantiate(&[
        0x41, 0x01, 0x42, 0x02, 0x41, 0x01, 0x1c, 0x01, 0x7f,
    ])
    .expect_err("declared i32 select must reject an i64 operand");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::TypeMismatch { .. })
    ));
}

#[test]
fn typed_select_rejects_non_singleton_type_vector() {
    for immediate in [&[0x00][..], &[0x02, 0x7f, 0x7f][..]] {
        let mut body = vec![0x41, 0x01, 0x41, 0x02, 0x41, 0x01, 0x1c];
        body.extend_from_slice(immediate);
        let error = instantiate(&body).expect_err("typed select requires one result type");
        assert!(matches!(
            error,
            RuntimeError::Validation(ValidationError::MalformedImmediate { .. })
        ));
    }
}

#[test]
fn typed_select_rejects_unsupported_result_type() {
    let error = instantiate(&[
        0x41, 0x01, 0x41, 0x02, 0x41, 0x01, 0x1c, 0x01, 0x6f,
    ])
    .expect_err("unsupported reference type must fail closed");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::MalformedImmediate { .. })
    ));
}
'''
Path("crates/wasm-runtime/tests/typed_select.rs").write_text(tests)

replace_once(
    "docs/core-control-ops.md",
    "- typed select (`0x1c`) remains unsupported.\n",
    "- typed select (`0x1c`) validates its singleton result-type vector and executes typed numeric or `funcref` selection, including nullable/non-null references.\n",
)
replace_once(
    "docs/phase5c-pinned-wast-parametric-control-tranche.md",
    "- typed `select` (`0x1c`) remains outside the runtime's documented surface\n",
    "- typed `select` (`0x1c`) is now executable, but this historical pinned tranche remains scoped to its existing untyped-select assertions; manifest accounting is unchanged in this slice\n",
)
