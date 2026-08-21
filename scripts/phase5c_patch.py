from pathlib import Path


def between(text: str, start: str, end: str, replacement: str) -> str:
    a = text.index(start)
    b = text.index(end, a)
    return text[:a] + replacement + text[b:]


# parser: reusable signed-33 LEB decoder for blocktype type indices
path = Path("crates/wasm-parser/src/lib.rs")
text = path.read_text()
if "pub fn decode_s33" not in text:
    marker = "\npub fn parse_module(bytes: &[u8]) -> Result<Module, ParseError> {"
    decoder = r'''
/// Decode a signed 33-bit LEB128 value used by WebAssembly block types.
///
/// The signed-33 domain can represent every u32 type index plus the negative
/// single-byte value-type encodings reserved by the binary format.
pub fn decode_s33(input: &[u8]) -> Result<(i64, usize), ParseError> {
    let mut result = 0i64;
    let mut shift = 0u32;
    for index in 0..5 {
        let byte = *input.get(index).ok_or(ParseError::UnexpectedEof)?;
        let payload = i64::from(byte & 0x7f);
        result |= payload << shift;
        shift += 7;
        if byte & 0x80 == 0 {
            if index == 4 {
                let unused = byte & 0x60;
                if unused != 0x00 && unused != 0x60 {
                    return Err(ParseError::Leb128Overflow);
                }
            }
            if byte & 0x40 != 0 && shift < 64 {
                result |= (!0i64) << shift;
            }
            const MIN_S33: i64 = -(1i64 << 32);
            const MAX_S33: i64 = (1i64 << 32) - 1;
            if !(MIN_S33..=MAX_S33).contains(&result) {
                return Err(ParseError::Leb128Overflow);
            }
            return Ok((result, index + 1));
        }
    }
    Err(ParseError::InvalidLeb128)
}
'''
    text = text.replace(marker, "\n" + decoder + marker, 1)
    path.write_text(text)


# validator errors for indexed block signatures
path = Path("crates/wasm-validator/src/lib.rs")
text = path.read_text()
if "BlockTypeIndexOutOfBounds" not in text:
    enum_marker = '''    UnsupportedBlockType {
        function: usize,
        offset: usize,
        block_type: u8,
    },
'''
    enum_insert = '''    BlockTypeIndexOutOfBounds {
        function: usize,
        offset: usize,
        type_index: u32,
    },
    UnsupportedBlockResultArity {
        function: usize,
        offset: usize,
        type_index: u32,
        results: usize,
    },
''' + enum_marker
    assert enum_marker in text
    text = text.replace(enum_marker, enum_insert, 1)

    display_start = text.index("impl fmt::Display for ValidationError")
    display_marker = '''            Self::UnsupportedBlockType {
                function,
                offset,
                block_type,
            } => write!(
'''
    pos = text.index(display_marker, display_start)
    display_insert = '''            Self::BlockTypeIndexOutOfBounds {
                function,
                offset,
                type_index,
            } => write!(
                f,
                "function {function} block at byte {offset} refers to missing type {type_index}"
            ),
            Self::UnsupportedBlockResultArity {
                function,
                offset,
                type_index,
                results,
            } => write!(
                f,
                "function {function} block at byte {offset} uses type {type_index} with {results} results; at most one is supported"
            ),
'''
    text = text[:pos] + display_insert + text[pos:]
    path.write_text(text)


# typed validator: full block signatures, including parameters
path = Path("crates/wasm-validator/src/typed.rs")
text = path.read_text()
if "struct BlockSignature" not in text:
    text = text.replace(
        "use wasm_parser::{decode_i32, decode_i64, FuncType, Module, ValueType};",
        "use wasm_parser::{decode_i32, decode_i64, decode_s33, FuncType, Module, ValueType};",
        1,
    )
    old_struct = '''#[derive(Debug, Clone, Copy)]
struct ControlFrame {
    kind: ControlKind,
    height: usize,
    end_type: Option<ValueType>,
    label_type: Option<ValueType>,
    unreachable: bool,
    seen_else: bool,
}
'''
    new_struct = '''#[derive(Debug, Clone)]
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
'''
    assert old_struct in text
    text = text.replace(old_struct, new_struct, 1)

    old_init = '''    let mut controls = vec![ControlFrame {
        kind: ControlKind::Function,
        height: 0,
        end_type: function_result,
        label_type: function_result,
        unreachable: false,
        seen_else: false,
    }];
'''
    new_init = '''    let mut controls = vec![ControlFrame {
        kind: ControlKind::Function,
        height: 0,
        param_types: Vec::new(),
        end_type: function_result,
        label_types: function_results.to_vec(),
        unreachable: false,
        seen_else: false,
    }];
'''
    assert old_init in text
    text = text.replace(old_init, new_init, 1)

    control_cases = r'''            0x02 | 0x03 => {
                let signature = read_block_signature(module, code, &mut pc, function, offset)?;
                let kind = if opcode == 0x02 {
                    ControlKind::Block
                } else {
                    ControlKind::Loop
                };
                let height = enter_control(
                    &mut stack,
                    &controls,
                    &signature.params,
                    function,
                    offset,
                )?;
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
                let height = enter_control(
                    &mut stack,
                    &controls,
                    &signature.params,
                    function,
                    offset,
                )?;
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
'''
    text = between(text, "            0x02 | 0x03 => {", "            0x10 => {", control_cases)

    helpers = r'''fn enter_control(
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

fn control_at_depth<'a>(
    controls: &'a [ControlFrame],
    depth: u32,
    function: usize,
    offset: usize,
) -> Result<&'a ControlFrame, ValidationError> {
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
    let type_index = u32::try_from(raw)
        .map_err(|_| ValidationError::MalformedImmediate { function, offset })?;
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

'''
    text = between(text, "fn require_label_value(", "fn read_local(", helpers)
    path.write_text(text)


# runtime: carry full block signatures through control-map + execution
path = Path("crates/wasm-runtime/src/lib.rs")
text = path.read_text()
if "struct BlockSignature" not in text:
    text = text.replace(
        "decode_i32, decode_i64, decode_u32, Constant, ExportKind, FuncType, Module, ParseError,",
        "decode_i32, decode_i64, decode_s33, decode_u32, Constant, ExportKind, FuncType, Module, ParseError,",
        1,
    )

    runtime_enum = "    UnsupportedBlockType(u8),\n"
    assert runtime_enum in text
    text = text.replace(
        runtime_enum,
        runtime_enum
        + "    BlockTypeIndexOutOfBounds(u32),\n"
        + "    UnsupportedBlockResultArity { type_index: u32, results: usize },\n",
        1,
    )

    display_marker = '''            Self::UnsupportedBlockType(block_type) => {
                write!(f, "unsupported block type 0x{block_type:02x}")
            }
'''
    assert display_marker in text
    text = text.replace(
        display_marker,
        display_marker
        + '''            Self::BlockTypeIndexOutOfBounds(type_index) => {
                write!(f, "block signature refers to missing type {type_index}")
            }
            Self::UnsupportedBlockResultArity { type_index, results } => write!(
                f,
                "block signature type {type_index} has {results} results; at most one is supported"
            ),
''',
        1,
    )

    structures = r'''#[derive(Debug, Clone, PartialEq, Eq)]
struct BlockSignature {
    params: Vec<ValueType>,
    result: Option<ValueType>,
}

#[derive(Debug, Clone)]
struct ControlInfo {
    kind: ControlKind,
    body_pc: usize,
    else_pc: Option<usize>,
    end_pc: usize,
    signature: BlockSignature,
}

#[derive(Debug, Clone)]
struct ControlMap {
    openers: Vec<Option<ControlInfo>>,
}

impl ControlMap {
    fn info(&self, opener: usize) -> Result<ControlInfo, RuntimeError> {
        self.openers
            .get(opener)
            .and_then(Clone::clone)
            .ok_or(RuntimeError::ControlInvariant(
                "structured-control opener has no boundary metadata",
            ))
    }
}

#[derive(Debug, Clone)]
struct PendingControl {
    opener: usize,
    kind: ControlKind,
    body_pc: usize,
    else_pc: Option<usize>,
    signature: BlockSignature,
}

#[derive(Debug, Clone)]
struct ExecControlFrame {
    kind: ControlKind,
    body_pc: usize,
    end_pc: usize,
    stack_height: usize,
    param_types: Vec<ValueType>,
    result_type: Option<ValueType>,
}

impl ExecControlFrame {
    fn label_types(&self) -> Vec<ValueType> {
        if self.kind == ControlKind::Loop {
            self.param_types.clone()
        } else {
            self.result_type.into_iter().collect()
        }
    }
}

'''
    text = between(
        text,
        "#[derive(Debug, Clone, Copy)]\nstruct ControlInfo",
        "#[derive(Debug, Clone, Copy)]\nstruct ExecutionBudget",
        structures,
    )

    text = text.replace(
        ".map(|body| build_control_map(&body.code))",
        ".map(|body| build_control_map(&module, &body.code))",
        1,
    )

    old_function_frame = '''        let mut controls = vec![ExecControlFrame {
            kind: ControlKind::Function,
            body_pc: 0,
            end_pc: function_end,
            stack_height: 0,
            result_type,
        }];
'''
    new_function_frame = '''        let mut controls = vec![ExecControlFrame {
            kind: ControlKind::Function,
            body_pc: 0,
            end_pc: function_end,
            stack_height: 0,
            param_types: Vec::new(),
            result_type,
        }];
'''
    assert old_function_frame in text
    text = text.replace(old_function_frame, new_function_frame, 1)

    runtime_control_cases = r'''                0x02 | 0x03 => {
                    let signature = read_block_signature(&self.module, code, &mut pc)?;
                    let info = control_map.info(offset)?;
                    let kind = if opcode == 0x02 {
                        ControlKind::Block
                    } else {
                        ControlKind::Loop
                    };
                    ensure_control_info(&info, kind, &signature)?;
                    let stack_height = control_entry_height(&stack, &signature.params)?;
                    controls.push(ExecControlFrame {
                        kind,
                        body_pc: info.body_pc,
                        end_pc: info.end_pc,
                        stack_height,
                        param_types: signature.params,
                        result_type: signature.result,
                    });
                }
                0x04 => {
                    let signature = read_block_signature(&self.module, code, &mut pc)?;
                    let condition = numeric::i32_from_stack(&mut stack)?;
                    let info = control_map.info(offset)?;
                    ensure_control_info(&info, ControlKind::If, &signature)?;
                    let stack_height = control_entry_height(&stack, &signature.params)?;
                    let frame = ExecControlFrame {
                        kind: ControlKind::If,
                        body_pc: info.body_pc,
                        end_pc: info.end_pc,
                        stack_height,
                        param_types: signature.params,
                        result_type: signature.result,
                    };
                    if condition != 0 {
                        controls.push(frame);
                    } else if let Some(else_pc) = info.else_pc {
                        controls.push(frame);
                        pc = else_pc + 1;
                    } else {
                        stack.truncate(frame.stack_height);
                        pc = info.end_pc + 1;
                    }
                }
'''
    text = between(text, "                0x02 | 0x03 => {", "                0x05 => {", runtime_control_cases)

    text = text.replace(
        "let frame = *controls.last().ok_or(RuntimeError::ControlInvariant(",
        "let frame = controls.last().cloned().ok_or(RuntimeError::ControlInvariant(",
    )

    helpers = r'''fn control_entry_height(stack: &[Value], params: &[ValueType]) -> Result<usize, RuntimeError> {
    if stack.len() < params.len() {
        return Err(RuntimeError::StackUnderflow);
    }
    let height = stack.len() - params.len();
    validate_values(params, &stack[height..])?;
    Ok(height)
}

fn ensure_control_info(
    info: &ControlInfo,
    kind: ControlKind,
    signature: &BlockSignature,
) -> Result<(), RuntimeError> {
    if info.kind != kind || &info.signature != signature {
        return Err(RuntimeError::ControlInvariant(
            "control metadata disagrees with instruction stream",
        ));
    }
    Ok(())
}

fn exit_control_frame(
    controls: &mut Vec<ExecControlFrame>,
    stack: &[Value],
) -> Result<(), RuntimeError> {
    let frame = controls.pop().ok_or(RuntimeError::ControlInvariant(
        "attempted to leave missing control frame",
    ))?;
    let expected = frame.stack_height + usize::from(frame.result_type.is_some());
    if stack.len() != expected {
        return Err(RuntimeError::ControlStackMismatch {
            expected,
            actual: stack.len(),
        });
    }
    if let Some(expected_type) = frame.result_type {
        let value = *stack.last().ok_or(RuntimeError::StackUnderflow)?;
        numeric::expect_type(value, expected_type)?;
    }
    Ok(())
}

fn branch_to(
    controls: &mut Vec<ExecControlFrame>,
    stack: &mut Vec<Value>,
    depth: u32,
    pc: &mut usize,
    code_len: usize,
) -> Result<(), RuntimeError> {
    let depth_usize = depth as usize;
    let target_index = controls
        .len()
        .checked_sub(depth_usize + 1)
        .ok_or(RuntimeError::BranchDepthOutOfBounds(depth))?;
    let target = controls[target_index].clone();
    let label_types = target.label_types();
    let label_arity = label_types.len();
    let current_height = controls
        .last()
        .map(|frame| frame.stack_height)
        .ok_or(RuntimeError::ControlInvariant(
            "branch executed without active control frame",
        ))?;
    if stack.len().saturating_sub(current_height) < label_arity {
        return Err(RuntimeError::StackUnderflow);
    }

    let label_values = stack[stack.len() - label_arity..].to_vec();
    validate_values(&label_types, &label_values)?;
    stack.truncate(target.stack_height);
    stack.extend(label_values);

    match target.kind {
        ControlKind::Loop => {
            controls.truncate(target_index + 1);
            *pc = target.body_pc;
        }
        ControlKind::Block | ControlKind::If => {
            controls.truncate(target_index);
            *pc = target.end_pc + 1;
        }
        ControlKind::Function => {
            controls.clear();
            *pc = code_len;
        }
    }
    Ok(())
}

fn build_control_map(module: &Module, code: &[u8]) -> Result<ControlMap, RuntimeError> {
    let mut openers = vec![None; code.len()];
    let mut pending = Vec::<PendingControl>::new();
    let mut pc = 0usize;

    while pc < code.len() {
        let offset = pc;
        let opcode = code[pc];
        pc += 1;
        match opcode {
            0x02..=0x04 => {
                let signature = read_block_signature(module, code, &mut pc)?;
                let kind = match opcode {
                    0x02 => ControlKind::Block,
                    0x03 => ControlKind::Loop,
                    0x04 => ControlKind::If,
                    _ => unreachable!(),
                };
                pending.push(PendingControl {
                    opener: offset,
                    kind,
                    body_pc: pc,
                    else_pc: None,
                    signature,
                });
            }
            0x05 => {
                let frame = pending.last_mut().ok_or(RuntimeError::ControlInvariant(
                    "else has no pending structured-control opener",
                ))?;
                if frame.kind != ControlKind::If || frame.else_pc.is_some() {
                    return Err(RuntimeError::ControlInvariant(
                        "else does not match exactly one if",
                    ));
                }
                frame.else_pc = Some(offset);
            }
            0x0b => {
                if let Some(frame) = pending.pop() {
                    openers[frame.opener] = Some(ControlInfo {
                        kind: frame.kind,
                        body_pc: frame.body_pc,
                        else_pc: frame.else_pc,
                        end_pc: offset,
                        signature: frame.signature,
                    });
                } else if pc != code.len() {
                    return Err(RuntimeError::ControlInvariant(
                        "function end occurs before final byte",
                    ));
                }
            }
            0x0c | 0x0d | 0x10 | 0x20..=0x24 | 0x3f | 0x40 => {
                let _ = read_u32_immediate(code, &mut pc)?;
            }
            0x11 => {
                let _ = read_u32_immediate(code, &mut pc)?;
                let _ = read_u32_immediate(code, &mut pc)?;
            }
            0x28 | 0x2c..=0x2f | 0x36 | 0x3a | 0x3b => {
                let _ = read_memarg(code, &mut pc)?;
            }
            0x41 => {
                let (_, used) = decode_i32(&code[pc..])?;
                pc += used;
            }
            0x42 => {
                let (_, used) = decode_i64(&code[pc..])?;
                pc += used;
            }
            0x43 => {
                let _ = read_fixed_u32(code, &mut pc)?;
            }
            0x44 => {
                let _ = read_fixed_u64(code, &mut pc)?;
            }
            0x0f
            | 0x45..=0x66
            | 0x6a..=0x6c
            | 0x7c..=0x7e
            | 0x92..=0x95
            | 0xa0..=0xa3
            | 0xa7
            | 0xac
            | 0xad
            | 0xb6
            | 0xbb => {}
            other => return Err(RuntimeError::UnsupportedOpcode(other)),
        }
    }

    if !pending.is_empty() {
        return Err(RuntimeError::ControlInvariant(
            "structured control is not fully closed",
        ));
    }
    Ok(ControlMap { openers })
}

fn read_block_signature(
    module: &Module,
    code: &[u8],
    pc: &mut usize,
) -> Result<BlockSignature, RuntimeError> {
    let first = *code
        .get(*pc)
        .ok_or(RuntimeError::ControlInvariant("missing block type"))?;
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

    let (raw, used) = decode_s33(&code[*pc..])?;
    *pc += used;
    if raw < 0 {
        return Err(RuntimeError::UnsupportedBlockType(first));
    }
    let type_index = u32::try_from(raw)
        .map_err(|_| RuntimeError::ControlInvariant("block type index exceeds u32"))?;
    let ty = module
        .types
        .get(type_index as usize)
        .ok_or(RuntimeError::BlockTypeIndexOutOfBounds(type_index))?;
    if ty.results.len() > 1 {
        return Err(RuntimeError::UnsupportedBlockResultArity {
            type_index,
            results: ty.results.len(),
        });
    }
    Ok(BlockSignature {
        params: ty.params.clone(),
        result: ty.results.first().copied(),
    })
}

'''
    text = between(text, "fn ensure_control_info(", "fn read_memarg(", helpers)
    path.write_text(text)
