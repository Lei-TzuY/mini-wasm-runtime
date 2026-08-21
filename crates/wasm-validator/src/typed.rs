use super::{function_type, ControlKind, ValidationError};
use wasm_parser::{decode_i32, decode_i64, decode_s33, FuncType, Module, ValueType};

#[derive(Debug, Clone)]
struct ControlFrame {
    kind: ControlKind,
    height: usize,
    param_types: Vec<ValueType>,
    end_type: Option<ValueType>,
    label_types: Vec<ValueType>,
    unreachable: bool,
    seen_else: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BlockSignature {
    params: Vec<ValueType>,
    result: Option<ValueType>,
}

pub(super) fn validate_code(
    module: &Module,
    body_index: usize,
    function: usize,
    local_types: &[ValueType],
    function_results: &[ValueType],
) -> Result<(), ValidationError> {
    let code = &module.code[body_index].code;
    if code.last().copied() != Some(0x0b) {
        return Err(ValidationError::MissingFunctionEnd { function });
    }

    let function_result = function_results.first().copied();
    let mut stack = Vec::<ValueType>::new();
    let mut controls = vec![ControlFrame {
        kind: ControlKind::Function,
        height: 0,
        param_types: Vec::new(),
        end_type: function_result,
        label_types: function_results.to_vec(),
        unreachable: false,
        seen_else: false,
    }];
    let mut pc = 0usize;

    while pc < code.len() {
        let offset = pc;
        let opcode = code[pc];
        pc += 1;

        match opcode {
            0x02 | 0x03 => {
                let signature = read_block_signature(module, code, &mut pc, function, offset)?;
                let kind = if opcode == 0x02 {
                    ControlKind::Block
                } else {
                    ControlKind::Loop
                };
                let height =
                    enter_control(&mut stack, &controls, &signature.params, function, offset)?;
                let label_types = if kind == ControlKind::Loop {
                    signature.params.clone()
                } else {
                    signature.result.into_iter().collect()
                };
                controls.push(ControlFrame {
                    kind,
                    height,
                    param_types: signature.params,
                    end_type: signature.result,
                    label_types,
                    unreachable: false,
                    seen_else: false,
                });
            }
            0x04 => {
                let signature = read_block_signature(module, code, &mut pc, function, offset)?;
                pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                let height =
                    enter_control(&mut stack, &controls, &signature.params, function, offset)?;
                let label_types = signature.result.into_iter().collect();
                controls.push(ControlFrame {
                    kind: ControlKind::If,
                    height,
                    param_types: signature.params,
                    end_type: signature.result,
                    label_types,
                    unreachable: false,
                    seen_else: false,
                });
            }
            0x05 => transition_to_else(&mut stack, &mut controls, function, offset)?,
            0x0b => {
                let frame = controls
                    .last()
                    .cloned()
                    .ok_or(ValidationError::UnexpectedEnd { function, offset })?;
                if frame.kind == ControlKind::If && frame.end_type.is_some() && !frame.seen_else {
                    return Err(ValidationError::MissingElseForResult { function, offset });
                }
                finish_frame(&mut stack, &frame, function, offset)?;
                controls.pop();
                if frame.kind == ControlKind::Function {
                    if pc != code.len() {
                        return Err(ValidationError::UnexpectedEnd { function, offset });
                    }
                } else {
                    stack.truncate(frame.height);
                    if let Some(ty) = frame.end_type {
                        stack.push(ty);
                    }
                }
            }
            0x0c => {
                let depth = read_u32(code, &mut pc, function, offset)?;
                require_label_values(&stack, &controls, depth, function, offset)?;
                mark_unreachable(&mut stack, &mut controls);
            }
            0x0d => {
                let depth = read_u32(code, &mut pc, function, offset)?;
                pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                require_label_values(&stack, &controls, depth, function, offset)?;
            }
            0x0f => {
                require_label_values(
                    &stack,
                    &controls,
                    (controls.len() - 1) as u32,
                    function,
                    offset,
                )?;
                mark_unreachable(&mut stack, &mut controls);
            }
            0x10 => {
                let target = read_u32(code, &mut pc, function, offset)?;
                let Some(ty) = function_type(module, target) else {
                    return Err(ValidationError::CallTargetOutOfBounds {
                        function,
                        offset,
                        target,
                    });
                };
                apply_call_signature(&mut stack, &controls, ty, function, offset)?;
            }
            0x11 => {
                let type_index = read_u32(code, &mut pc, function, offset)?;
                let table_index = read_u32(code, &mut pc, function, offset)?;
                if table_index as usize >= module.table_count() {
                    return Err(ValidationError::TableIndexOutOfBounds {
                        function,
                        offset,
                        table_index,
                    });
                }
                let Some(ty) = module.types.get(type_index as usize) else {
                    return Err(ValidationError::IndirectTypeIndexOutOfBounds {
                        function,
                        offset,
                        type_index,
                    });
                };
                if ty.results.len() > 1 {
                    return Err(ValidationError::UnsupportedIndirectResultArity {
                        function,
                        offset,
                        results: ty.results.len(),
                    });
                }
                pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                apply_call_signature(&mut stack, &controls, ty, function, offset)?;
            }
            0x20 => {
                let index = read_local(code, &mut pc, function, offset, local_types)?;
                let ty = local_types[index as usize];
                stack.push(ty);
            }
            0x21 => {
                let index = read_local(code, &mut pc, function, offset, local_types)?;
                let ty = local_types[index as usize];
                pop_expect(&mut stack, &controls, ty, function, offset)?;
            }
            0x22 => {
                let index = read_local(code, &mut pc, function, offset, local_types)?;
                let ty = local_types[index as usize];
                peek_expect(&stack, &controls, ty, function, offset)?;
            }
            0x23 => {
                let global_index = read_u32(code, &mut pc, function, offset)?;
                let Some(global_type) = module.global_type(global_index) else {
                    return Err(ValidationError::GlobalIndexOutOfBounds {
                        function,
                        offset,
                        global_index,
                    });
                };
                stack.push(global_type.value_type);
            }
            0x24 => {
                let global_index = read_u32(code, &mut pc, function, offset)?;
                let Some(global_type) = module.global_type(global_index) else {
                    return Err(ValidationError::GlobalIndexOutOfBounds {
                        function,
                        offset,
                        global_index,
                    });
                };
                if !global_type.mutable {
                    return Err(ValidationError::ImmutableGlobalSet {
                        function,
                        offset,
                        global_index,
                    });
                }
                pop_expect(
                    &mut stack,
                    &controls,
                    global_type.value_type,
                    function,
                    offset,
                )?;
            }
            0x28..=0x35 => {
                super::ensure_memory(module, function, offset)?;
                super::read_memarg(
                    code,
                    &mut pc,
                    function,
                    offset,
                    super::natural_alignment(opcode),
                )?;
                pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                let result = match opcode {
                    0x28 | 0x2c..=0x2f => ValueType::I32,
                    0x29 | 0x30..=0x35 => ValueType::I64,
                    0x2a => ValueType::F32,
                    0x2b => ValueType::F64,
                    _ => unreachable!(),
                };
                stack.push(result);
            }
            0x36..=0x3e => {
                super::ensure_memory(module, function, offset)?;
                super::read_memarg(
                    code,
                    &mut pc,
                    function,
                    offset,
                    super::natural_alignment(opcode),
                )?;
                let value_type = match opcode {
                    0x36 | 0x3a | 0x3b => ValueType::I32,
                    0x37 | 0x3c..=0x3e => ValueType::I64,
                    0x38 => ValueType::F32,
                    0x39 => ValueType::F64,
                    _ => unreachable!(),
                };
                pop_expect(&mut stack, &controls, value_type, function, offset)?;
                pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
            }
            0x3f => {
                super::read_memory_index(code, &mut pc, module, function, offset)?;
                stack.push(ValueType::I32);
            }
            0x40 => {
                super::read_memory_index(code, &mut pc, module, function, offset)?;
                pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                stack.push(ValueType::I32);
            }
            0x41 => {
                skip_i32(code, &mut pc, function, offset)?;
                stack.push(ValueType::I32);
            }
            0x42 => {
                skip_i64(code, &mut pc, function, offset)?;
                stack.push(ValueType::I64);
            }
            0x43 => {
                skip_fixed(code, &mut pc, 4, function, offset)?;
                stack.push(ValueType::F32);
            }
            0x44 => {
                skip_fixed(code, &mut pc, 8, function, offset)?;
                stack.push(ValueType::F64);
            }
            0x45 => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::I32,
                    ValueType::I32,
                    function,
                    offset,
                )?;
            }
            0x46..=0x4f => {
                binary_compare(&mut stack, &controls, ValueType::I32, function, offset)?;
            }
            0x50 => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::I64,
                    ValueType::I32,
                    function,
                    offset,
                )?;
            }
            0x51..=0x5a => {
                binary_compare(&mut stack, &controls, ValueType::I64, function, offset)?;
            }
            0x5b..=0x60 => {
                binary_compare(&mut stack, &controls, ValueType::F32, function, offset)?;
            }
            0x61..=0x66 => {
                binary_compare(&mut stack, &controls, ValueType::F64, function, offset)?;
            }
            0x67..=0x69 => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::I32,
                    ValueType::I32,
                    function,
                    offset,
                )?;
            }
            0x6a..=0x78 => binary_same(&mut stack, &controls, ValueType::I32, function, offset)?,
            0x79..=0x7b => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::I64,
                    ValueType::I64,
                    function,
                    offset,
                )?;
            }
            0x7c..=0x8a => {
                binary_same(&mut stack, &controls, ValueType::I64, function, offset)?;
            }
            0x8b..=0x91 => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::F32,
                    ValueType::F32,
                    function,
                    offset,
                )?;
            }
            0x92..=0x98 => {
                binary_same(&mut stack, &controls, ValueType::F32, function, offset)?;
            }
            0x99..=0x9f => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::F64,
                    ValueType::F64,
                    function,
                    offset,
                )?;
            }
            0xa0..=0xa6 => {
                binary_same(&mut stack, &controls, ValueType::F64, function, offset)?;
            }
            0xa7 => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::I64,
                    ValueType::I32,
                    function,
                    offset,
                )?;
            }
            0xac | 0xad => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::I32,
                    ValueType::I64,
                    function,
                    offset,
                )?;
            }
            0xb6 => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::F64,
                    ValueType::F32,
                    function,
                    offset,
                )?;
            }
            0xbb => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::F32,
                    ValueType::F64,
                    function,
                    offset,
                )?;
            }
            0xbc => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::F32,
                    ValueType::I32,
                    function,
                    offset,
                )?;
            }
            0xbd => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::F64,
                    ValueType::I64,
                    function,
                    offset,
                )?;
            }
            0xbe => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::I32,
                    ValueType::F32,
                    function,
                    offset,
                )?;
            }
            0xbf => {
                unary(
                    &mut stack,
                    &controls,
                    ValueType::I64,
                    ValueType::F64,
                    function,
                    offset,
                )?;
            }
            other => {
                return Err(ValidationError::UnsupportedOpcode {
                    function,
                    offset,
                    opcode: other,
                });
            }
        }
    }

    if !controls.is_empty() {
        return Err(ValidationError::MissingFunctionEnd { function });
    }
    Ok(())
}

fn apply_call_signature(
    stack: &mut Vec<ValueType>,
    controls: &[ControlFrame],
    ty: &FuncType,
    function: usize,
    offset: usize,
) -> Result<(), ValidationError> {
    for &param in ty.params.iter().rev() {
        pop_expect(stack, controls, param, function, offset)?;
    }
    if let Some(&result) = ty.results.first() {
        stack.push(result);
    }
    Ok(())
}

fn unary(
    stack: &mut Vec<ValueType>,
    controls: &[ControlFrame],
    input: ValueType,
    output: ValueType,
    function: usize,
    offset: usize,
) -> Result<(), ValidationError> {
    pop_expect(stack, controls, input, function, offset)?;
    stack.push(output);
    Ok(())
}

fn binary_same(
    stack: &mut Vec<ValueType>,
    controls: &[ControlFrame],
    ty: ValueType,
    function: usize,
    offset: usize,
) -> Result<(), ValidationError> {
    pop_expect(stack, controls, ty, function, offset)?;
    pop_expect(stack, controls, ty, function, offset)?;
    stack.push(ty);
    Ok(())
}

fn binary_compare(
    stack: &mut Vec<ValueType>,
    controls: &[ControlFrame],
    ty: ValueType,
    function: usize,
    offset: usize,
) -> Result<(), ValidationError> {
    pop_expect(stack, controls, ty, function, offset)?;
    pop_expect(stack, controls, ty, function, offset)?;
    stack.push(ValueType::I32);
    Ok(())
}

fn pop_expect(
    stack: &mut Vec<ValueType>,
    controls: &[ControlFrame],
    expected: ValueType,
    function: usize,
    offset: usize,
) -> Result<(), ValidationError> {
    let frame = controls
        .last()
        .ok_or(ValidationError::OperandStackUnderflow { function, offset })?;
    if stack.len() == frame.height && frame.unreachable {
        return Ok(());
    }
    let actual = stack
        .pop()
        .ok_or(ValidationError::OperandStackUnderflow { function, offset })?;
    if actual != expected {
        return Err(ValidationError::TypeMismatch {
            function,
            offset,
            expected,
            actual,
        });
    }
    Ok(())
}

fn peek_expect(
    stack: &[ValueType],
    controls: &[ControlFrame],
    expected: ValueType,
    function: usize,
    offset: usize,
) -> Result<(), ValidationError> {
    let frame = controls
        .last()
        .ok_or(ValidationError::OperandStackUnderflow { function, offset })?;
    if stack.len() == frame.height && frame.unreachable {
        return Ok(());
    }
    let actual = *stack
        .last()
        .ok_or(ValidationError::OperandStackUnderflow { function, offset })?;
    if actual != expected {
        return Err(ValidationError::TypeMismatch {
            function,
            offset,
            expected,
            actual,
        });
    }
    Ok(())
}

fn enter_control(
    stack: &mut Vec<ValueType>,
    controls: &[ControlFrame],
    params: &[ValueType],
    function: usize,
    offset: usize,
) -> Result<usize, ValidationError> {
    for &param in params.iter().rev() {
        pop_expect(stack, controls, param, function, offset)?;
    }
    let height = stack.len();
    stack.extend(params.iter().copied());
    Ok(height)
}

fn require_label_values(
    stack: &[ValueType],
    controls: &[ControlFrame],
    depth: u32,
    function: usize,
    offset: usize,
) -> Result<(), ValidationError> {
    let target = control_at_depth(controls, depth, function, offset)?;
    let current = controls
        .last()
        .ok_or(ValidationError::OperandStackUnderflow { function, offset })?;
    if current.unreachable && stack.len() == current.height {
        return Ok(());
    }
    if stack.len().saturating_sub(current.height) < target.label_types.len() {
        return Err(ValidationError::OperandStackUnderflow { function, offset });
    }
    let start = stack.len() - target.label_types.len();
    for (&actual, &expected) in stack[start..].iter().zip(&target.label_types) {
        if actual != expected {
            return Err(ValidationError::TypeMismatch {
                function,
                offset,
                expected,
                actual,
            });
        }
    }
    Ok(())
}

fn control_at_depth(
    controls: &[ControlFrame],
    depth: u32,
    function: usize,
    offset: usize,
) -> Result<&ControlFrame, ValidationError> {
    let index = controls.len().checked_sub(depth as usize + 1).ok_or(
        ValidationError::BranchDepthOutOfBounds {
            function,
            offset,
            depth,
        },
    )?;
    Ok(&controls[index])
}

fn mark_unreachable(stack: &mut Vec<ValueType>, controls: &mut [ControlFrame]) {
    if let Some(frame) = controls.last_mut() {
        stack.truncate(frame.height);
        frame.unreachable = true;
    }
}

fn transition_to_else(
    stack: &mut Vec<ValueType>,
    controls: &mut [ControlFrame],
    function: usize,
    offset: usize,
) -> Result<(), ValidationError> {
    let frame = controls
        .last()
        .cloned()
        .ok_or(ValidationError::UnexpectedElse { function, offset })?;
    if frame.kind != ControlKind::If {
        return Err(ValidationError::UnexpectedElse { function, offset });
    }
    if frame.seen_else {
        return Err(ValidationError::DuplicateElse { function, offset });
    }
    finish_frame(stack, &frame, function, offset)?;
    stack.truncate(frame.height);
    stack.extend(frame.param_types.iter().copied());
    let current = controls
        .last_mut()
        .ok_or(ValidationError::UnexpectedElse { function, offset })?;
    current.unreachable = false;
    current.seen_else = true;
    Ok(())
}

fn finish_frame(
    stack: &mut Vec<ValueType>,
    frame: &ControlFrame,
    function: usize,
    offset: usize,
) -> Result<(), ValidationError> {
    if frame.unreachable {
        stack.truncate(frame.height);
        return Ok(());
    }
    let expected = frame.height + usize::from(frame.end_type.is_some());
    if stack.len() != expected {
        return Err(ValidationError::StackHeightMismatch {
            function,
            offset,
            expected,
            actual: stack.len(),
        });
    }
    if let Some(expected_type) = frame.end_type {
        let actual = stack[frame.height];
        if actual != expected_type {
            return Err(ValidationError::TypeMismatch {
                function,
                offset,
                expected: expected_type,
                actual,
            });
        }
    }
    Ok(())
}

fn read_block_signature(
    module: &Module,
    code: &[u8],
    pc: &mut usize,
    function: usize,
    offset: usize,
) -> Result<BlockSignature, ValidationError> {
    let first = *code
        .get(*pc)
        .ok_or(ValidationError::MalformedImmediate { function, offset })?;
    let immediate = match first {
        0x40 => {
            *pc += 1;
            return Ok(BlockSignature {
                params: Vec::new(),
                result: None,
            });
        }
        0x7f => Some(ValueType::I32),
        0x7e => Some(ValueType::I64),
        0x7d => Some(ValueType::F32),
        0x7c => Some(ValueType::F64),
        _ => None,
    };
    if let Some(result) = immediate {
        *pc += 1;
        return Ok(BlockSignature {
            params: Vec::new(),
            result: Some(result),
        });
    }

    let (raw, used) = decode_s33(&code[*pc..])
        .map_err(|_| ValidationError::MalformedImmediate { function, offset })?;
    *pc += used;
    if raw < 0 {
        return Err(ValidationError::UnsupportedBlockType {
            function,
            offset,
            block_type: first,
        });
    }
    let type_index =
        u32::try_from(raw).map_err(|_| ValidationError::MalformedImmediate { function, offset })?;
    let ty = module.types.get(type_index as usize).ok_or(
        ValidationError::BlockTypeIndexOutOfBounds {
            function,
            offset,
            type_index,
        },
    )?;
    if ty.results.len() > 1 {
        return Err(ValidationError::UnsupportedBlockResultArity {
            function,
            offset,
            type_index,
            results: ty.results.len(),
        });
    }
    Ok(BlockSignature {
        params: ty.params.clone(),
        result: ty.results.first().copied(),
    })
}

fn read_local(
    code: &[u8],
    pc: &mut usize,
    function: usize,
    offset: usize,
    locals: &[ValueType],
) -> Result<u32, ValidationError> {
    let index = read_u32(code, pc, function, offset)?;
    if index as usize >= locals.len() {
        return Err(ValidationError::LocalIndexOutOfBounds {
            function,
            offset,
            local_index: index,
        });
    }
    Ok(index)
}

fn read_u32(
    code: &[u8],
    pc: &mut usize,
    function: usize,
    offset: usize,
) -> Result<u32, ValidationError> {
    let (value, used) = wasm_parser::decode_u32(&code[*pc..])
        .map_err(|_| ValidationError::MalformedImmediate { function, offset })?;
    *pc += used;
    Ok(value)
}

fn skip_i32(
    code: &[u8],
    pc: &mut usize,
    function: usize,
    offset: usize,
) -> Result<(), ValidationError> {
    let (_, used) = decode_i32(&code[*pc..])
        .map_err(|_| ValidationError::MalformedImmediate { function, offset })?;
    *pc += used;
    Ok(())
}

fn skip_i64(
    code: &[u8],
    pc: &mut usize,
    function: usize,
    offset: usize,
) -> Result<(), ValidationError> {
    let (_, used) = decode_i64(&code[*pc..])
        .map_err(|_| ValidationError::MalformedImmediate { function, offset })?;
    *pc += used;
    Ok(())
}

fn skip_fixed(
    code: &[u8],
    pc: &mut usize,
    width: usize,
    function: usize,
    offset: usize,
) -> Result<(), ValidationError> {
    let end = (*pc)
        .checked_add(width)
        .filter(|end| *end <= code.len())
        .ok_or(ValidationError::MalformedImmediate { function, offset })?;
    *pc = end;
    Ok(())
}
