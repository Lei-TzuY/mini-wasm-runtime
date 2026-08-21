//! Stack interpreter for the Phase-2 WebAssembly subset.

use std::fmt;
use wasm_parser::{decode_i32, decode_u32, ExportKind, Module, ParseError, ValueType};
use wasm_validator::{validate, ValidationError};

const MAX_CALL_DEPTH: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value {
    I32(i32),
}

impl Value {
    pub fn as_i32(self) -> i32 {
        match self {
            Self::I32(value) => value,
        }
    }
}

#[derive(Debug)]
pub enum RuntimeError {
    Validation(ValidationError),
    Decode(ParseError),
    ExportNotFound(String),
    ExportNotFunction(String),
    FunctionOutOfBounds(u32),
    UnsupportedType(ValueType),
    WrongArgumentCount { expected: usize, actual: usize },
    LocalOutOfBounds(u32),
    StackUnderflow,
    UnsupportedOpcode(u8),
    UnsupportedBlockType(u8),
    BranchDepthOutOfBounds(u32),
    ControlStackMismatch { expected: usize, actual: usize },
    ControlInvariant(&'static str),
    ResultArityMismatch { expected: usize, actual: usize },
    CallDepthExceeded,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(error) => write!(f, "validation failed: {error}"),
            Self::Decode(error) => write!(f, "instruction decode failed: {error}"),
            Self::ExportNotFound(name) => write!(f, "export {name:?} not found"),
            Self::ExportNotFunction(name) => write!(f, "export {name:?} is not a function"),
            Self::FunctionOutOfBounds(index) => {
                write!(f, "function index {index} is out of bounds")
            }
            Self::UnsupportedType(ty) => write!(f, "runtime does not yet execute type {ty:?}"),
            Self::WrongArgumentCount { expected, actual } => {
                write!(f, "expected {expected} arguments, got {actual}")
            }
            Self::LocalOutOfBounds(index) => write!(f, "local index {index} is out of bounds"),
            Self::StackUnderflow => write!(f, "operand stack underflow"),
            Self::UnsupportedOpcode(opcode) => write!(f, "unsupported opcode 0x{opcode:02x}"),
            Self::UnsupportedBlockType(block_type) => {
                write!(f, "unsupported block type 0x{block_type:02x}")
            }
            Self::BranchDepthOutOfBounds(depth) => {
                write!(f, "branch label depth {depth} is out of bounds")
            }
            Self::ControlStackMismatch { expected, actual } => write!(
                f,
                "control frame expects stack height {expected}, got {actual}"
            ),
            Self::ControlInvariant(message) => write!(f, "validated control invariant failed: {message}"),
            Self::ResultArityMismatch { expected, actual } => {
                write!(
                    f,
                    "expected {expected} result values, stack contains {actual}"
                )
            }
            Self::CallDepthExceeded => write!(f, "maximum call depth exceeded"),
        }
    }
}

impl std::error::Error for RuntimeError {}

impl From<ValidationError> for RuntimeError {
    fn from(value: ValidationError) -> Self {
        Self::Validation(value)
    }
}

impl From<ParseError> for RuntimeError {
    fn from(value: ParseError) -> Self {
        Self::Decode(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ControlKind {
    Function,
    Block,
    Loop,
    If,
}

#[derive(Debug, Clone, Copy)]
struct ControlInfo {
    kind: ControlKind,
    body_pc: usize,
    else_pc: Option<usize>,
    end_pc: usize,
    result_arity: usize,
}

#[derive(Debug, Clone)]
struct ControlMap {
    openers: Vec<Option<ControlInfo>>,
}

impl ControlMap {
    fn info(&self, opener: usize) -> Result<ControlInfo, RuntimeError> {
        self.openers
            .get(opener)
            .and_then(|info| *info)
            .ok_or(RuntimeError::ControlInvariant(
                "structured-control opener has no boundary metadata",
            ))
    }
}

#[derive(Debug, Clone, Copy)]
struct PendingControl {
    opener: usize,
    kind: ControlKind,
    body_pc: usize,
    else_pc: Option<usize>,
    result_arity: usize,
}

#[derive(Debug, Clone, Copy)]
struct ExecControlFrame {
    kind: ControlKind,
    body_pc: usize,
    end_pc: usize,
    stack_height: usize,
    result_arity: usize,
}

impl ExecControlFrame {
    fn label_arity(self) -> usize {
        if self.kind == ControlKind::Loop {
            0
        } else {
            self.result_arity
        }
    }
}

#[derive(Debug)]
pub struct Instance {
    module: Module,
    control_maps: Vec<ControlMap>,
}

impl Instance {
    pub fn new(module: Module) -> Result<Self, RuntimeError> {
        validate(&module)?;
        let control_maps = module
            .code
            .iter()
            .map(|body| build_control_map(&body.code))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            module,
            control_maps,
        })
    }

    pub fn invoke_export(&self, name: &str, args: &[Value]) -> Result<Option<Value>, RuntimeError> {
        let export = self
            .module
            .exports
            .iter()
            .find(|export| export.name == name)
            .ok_or_else(|| RuntimeError::ExportNotFound(name.to_owned()))?;
        if export.kind != ExportKind::Function {
            return Err(RuntimeError::ExportNotFunction(name.to_owned()));
        }
        self.invoke_function(export.index, args, 0)
    }

    fn invoke_function(
        &self,
        function_index: u32,
        args: &[Value],
        depth: usize,
    ) -> Result<Option<Value>, RuntimeError> {
        if depth >= MAX_CALL_DEPTH {
            return Err(RuntimeError::CallDepthExceeded);
        }

        let function = function_index as usize;
        let type_index = *self
            .module
            .function_type_indices
            .get(function)
            .ok_or(RuntimeError::FunctionOutOfBounds(function_index))?
            as usize;
        let ty = &self.module.types[type_index];
        ensure_i32_types(&ty.params)?;
        ensure_i32_types(&ty.results)?;

        if args.len() != ty.params.len() {
            return Err(RuntimeError::WrongArgumentCount {
                expected: ty.params.len(),
                actual: args.len(),
            });
        }

        let body = &self.module.code[function];
        let control_map = &self.control_maps[function];
        let mut locals = args.to_vec();
        for &(count, local_type) in &body.locals {
            if local_type != ValueType::I32 {
                return Err(RuntimeError::UnsupportedType(local_type));
            }
            locals.extend(std::iter::repeat(Value::I32(0)).take(count as usize));
        }

        let mut stack = Vec::new();
        let mut pc = 0usize;
        let code = &body.code;
        let result_arity = ty.results.len();
        let function_end = code
            .len()
            .checked_sub(1)
            .ok_or(RuntimeError::ControlInvariant("function body is empty"))?;
        let mut controls = vec![ExecControlFrame {
            kind: ControlKind::Function,
            body_pc: 0,
            end_pc: function_end,
            stack_height: 0,
            result_arity,
        }];

        while pc < code.len() {
            let offset = pc;
            let opcode = code[pc];
            pc += 1;

            match opcode {
                0x02 | 0x03 => {
                    let result_arity = read_block_arity(code, &mut pc)?;
                    let info = control_map.info(offset)?;
                    let kind = if opcode == 0x02 {
                        ControlKind::Block
                    } else {
                        ControlKind::Loop
                    };
                    ensure_control_info(info, kind, result_arity)?;
                    controls.push(ExecControlFrame {
                        kind,
                        body_pc: info.body_pc,
                        end_pc: info.end_pc,
                        stack_height: stack.len(),
                        result_arity,
                    });
                }
                0x04 => {
                    let result_arity = read_block_arity(code, &mut pc)?;
                    let condition = stack.pop().ok_or(RuntimeError::StackUnderflow)?.as_i32();
                    let info = control_map.info(offset)?;
                    ensure_control_info(info, ControlKind::If, result_arity)?;
                    let frame = ExecControlFrame {
                        kind: ControlKind::If,
                        body_pc: info.body_pc,
                        end_pc: info.end_pc,
                        stack_height: stack.len(),
                        result_arity,
                    };

                    if condition != 0 {
                        controls.push(frame);
                    } else if let Some(else_pc) = info.else_pc {
                        controls.push(frame);
                        pc = else_pc + 1;
                    } else {
                        pc = info.end_pc + 1;
                    }
                }
                0x05 => {
                    let frame = *controls.last().ok_or(RuntimeError::ControlInvariant(
                        "else encountered without active control frame",
                    ))?;
                    if frame.kind != ControlKind::If {
                        return Err(RuntimeError::ControlInvariant(
                            "else encountered outside active if",
                        ));
                    }
                    exit_control_frame(&mut controls, &mut stack)?;
                    pc = frame.end_pc + 1;
                }
                0x0b => {
                    let frame = *controls.last().ok_or(RuntimeError::ControlInvariant(
                        "end encountered without active control frame",
                    ))?;
                    if frame.end_pc != offset {
                        return Err(RuntimeError::ControlInvariant(
                            "end offset does not match active control frame",
                        ));
                    }
                    exit_control_frame(&mut controls, &mut stack)?;
                    if frame.kind == ControlKind::Function {
                        break;
                    }
                }
                0x0c => {
                    let branch_depth = read_u32_immediate(code, &mut pc)?;
                    branch_to(
                        &mut controls,
                        &mut stack,
                        branch_depth,
                        &mut pc,
                        code.len(),
                    )?;
                }
                0x0d => {
                    let branch_depth = read_u32_immediate(code, &mut pc)?;
                    let condition = stack.pop().ok_or(RuntimeError::StackUnderflow)?.as_i32();
                    if condition != 0 {
                        branch_to(
                            &mut controls,
                            &mut stack,
                            branch_depth,
                            &mut pc,
                            code.len(),
                        )?;
                    }
                }
                0x0f => {
                    let branch_depth = controls
                        .len()
                        .checked_sub(1)
                        .ok_or(RuntimeError::ControlInvariant(
                            "return executed without function frame",
                        ))? as u32;
                    branch_to(
                        &mut controls,
                        &mut stack,
                        branch_depth,
                        &mut pc,
                        code.len(),
                    )?;
                }
                0x10 => {
                    let callee = read_u32_immediate(code, &mut pc)?;
                    let callee_type_index = *self
                        .module
                        .function_type_indices
                        .get(callee as usize)
                        .ok_or(RuntimeError::FunctionOutOfBounds(callee))?
                        as usize;
                    let param_count = self.module.types[callee_type_index].params.len();
                    if stack.len() < param_count {
                        return Err(RuntimeError::StackUnderflow);
                    }
                    let call_args = stack.split_off(stack.len() - param_count);
                    if let Some(result) = self.invoke_function(callee, &call_args, depth + 1)? {
                        stack.push(result);
                    }
                }
                0x20 => {
                    let index = read_u32_immediate(code, &mut pc)?;
                    let value = *locals
                        .get(index as usize)
                        .ok_or(RuntimeError::LocalOutOfBounds(index))?;
                    stack.push(value);
                }
                0x21 => {
                    let index = read_u32_immediate(code, &mut pc)?;
                    let value = stack.pop().ok_or(RuntimeError::StackUnderflow)?;
                    let local = locals
                        .get_mut(index as usize)
                        .ok_or(RuntimeError::LocalOutOfBounds(index))?;
                    *local = value;
                }
                0x22 => {
                    let index = read_u32_immediate(code, &mut pc)?;
                    let value = *stack.last().ok_or(RuntimeError::StackUnderflow)?;
                    let local = locals
                        .get_mut(index as usize)
                        .ok_or(RuntimeError::LocalOutOfBounds(index))?;
                    *local = value;
                }
                0x41 => {
                    let (value, used) = decode_i32(&code[pc..])?;
                    pc += used;
                    stack.push(Value::I32(value));
                }
                0x6a => binary_i32(&mut stack, i32::wrapping_add)?,
                0x6b => binary_i32(&mut stack, i32::wrapping_sub)?,
                0x6c => binary_i32(&mut stack, i32::wrapping_mul)?,
                other => return Err(RuntimeError::UnsupportedOpcode(other)),
            }
        }

        if stack.len() != result_arity {
            return Err(RuntimeError::ResultArityMismatch {
                expected: result_arity,
                actual: stack.len(),
            });
        }
        Ok(stack.pop())
    }
}

fn ensure_i32_types(types: &[ValueType]) -> Result<(), RuntimeError> {
    for &ty in types {
        if ty != ValueType::I32 {
            return Err(RuntimeError::UnsupportedType(ty));
        }
    }
    Ok(())
}

fn ensure_control_info(
    info: ControlInfo,
    kind: ControlKind,
    result_arity: usize,
) -> Result<(), RuntimeError> {
    if info.kind != kind || info.result_arity != result_arity {
        return Err(RuntimeError::ControlInvariant(
            "control metadata disagrees with instruction stream",
        ));
    }
    Ok(())
}

fn exit_control_frame(
    controls: &mut Vec<ExecControlFrame>,
    stack: &mut Vec<Value>,
) -> Result<(), RuntimeError> {
    let frame = controls.pop().ok_or(RuntimeError::ControlInvariant(
        "attempted to leave missing control frame",
    ))?;
    let expected = frame.stack_height + frame.result_arity;
    if stack.len() != expected {
        return Err(RuntimeError::ControlStackMismatch {
            expected,
            actual: stack.len(),
        });
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
    let target = controls[target_index];
    let label_arity = target.label_arity();
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

fn build_control_map(code: &[u8]) -> Result<ControlMap, RuntimeError> {
    let mut openers = vec![None; code.len()];
    let mut pending = Vec::<PendingControl>::new();
    let mut pc = 0usize;

    while pc < code.len() {
        let offset = pc;
        let opcode = code[pc];
        pc += 1;
        match opcode {
            0x02 | 0x03 | 0x04 => {
                let result_arity = read_block_arity(code, &mut pc)?;
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
                    result_arity,
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
                        result_arity: frame.result_arity,
                    });
                } else if pc != code.len() {
                    return Err(RuntimeError::ControlInvariant(
                        "function end occurs before final byte",
                    ));
                }
            }
            0x0c | 0x0d | 0x10 | 0x20..=0x22 => {
                let _ = read_u32_immediate(code, &mut pc)?;
            }
            0x41 => {
                let (_, used) = decode_i32(&code[pc..])?;
                pc += used;
            }
            0x0f | 0x6a..=0x6c => {}
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

fn read_block_arity(code: &[u8], pc: &mut usize) -> Result<usize, RuntimeError> {
    let block_type = *code
        .get(*pc)
        .ok_or(RuntimeError::ControlInvariant("missing block type"))?;
    *pc += 1;
    match block_type {
        0x40 => Ok(0),
        0x7f => Ok(1),
        other => Err(RuntimeError::UnsupportedBlockType(other)),
    }
}

fn read_u32_immediate(code: &[u8], pc: &mut usize) -> Result<u32, RuntimeError> {
    let (value, used) = decode_u32(&code[*pc..])?;
    *pc += used;
    Ok(value)
}

fn binary_i32(stack: &mut Vec<Value>, operation: fn(i32, i32) -> i32) -> Result<(), RuntimeError> {
    let rhs = stack.pop().ok_or(RuntimeError::StackUnderflow)?.as_i32();
    let lhs = stack.pop().ok_or(RuntimeError::StackUnderflow)?.as_i32();
    stack.push(Value::I32(operation(lhs, rhs)));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_parser::parse_module;

    fn module_with_body(params: u8, results: u8, body: &[u8]) -> Vec<u8> {
        let type_payload = [
            0x01, 0x60, params, // one function type + parameter count
        ];
        let mut type_section = type_payload.to_vec();
        type_section.extend(std::iter::repeat(0x7f).take(params as usize));
        type_section.push(results);
        type_section.extend(std::iter::repeat(0x7f).take(results as usize));

        let mut code_payload = vec![0x01, (body.len() + 1) as u8, 0x00];
        code_payload.extend(body);

        let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        bytes.extend([0x01, type_section.len() as u8]);
        bytes.extend(type_section);
        bytes.extend([0x03, 0x02, 0x01, 0x00]);
        bytes.extend([0x07, 0x07, 0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);
        bytes.extend([0x0a, code_payload.len() as u8]);
        bytes.extend(code_payload);
        bytes
    }

    fn instance(bytes: &[u8]) -> Instance {
        Instance::new(parse_module(bytes).expect("parse test module"))
            .expect("validate test module")
    }

    #[test]
    fn executes_i32_add() {
        let bytes = module_with_body(2, 1, &[0x20, 0x00, 0x20, 0x01, 0x6a, 0x0b]);
        let result = instance(&bytes)
            .invoke_export("run", &[Value::I32(20), Value::I32(22)])
            .expect("execution succeeds");
        assert_eq!(result, Some(Value::I32(42)));
    }

    #[test]
    fn integer_arithmetic_wraps_like_wasm() {
        let bytes = module_with_body(1, 1, &[0x20, 0x00, 0x41, 0x01, 0x6a, 0x0b]);
        let result = instance(&bytes)
            .invoke_export("run", &[Value::I32(i32::MAX)])
            .expect("execution succeeds");
        assert_eq!(result, Some(Value::I32(i32::MIN)));
    }

    #[test]
    fn executes_if_else_on_both_paths() {
        let bytes = module_with_body(
            1,
            1,
            &[
                0x20, 0x00, // local.get 0
                0x04, 0x7f, // if (result i32)
                0x41, 0x0b, // i32.const 11
                0x05, // else
                0x41, 0x16, // i32.const 22
                0x0b, // end if
                0x0b, // end function
            ],
        );
        let vm = instance(&bytes);
        assert_eq!(
            vm.invoke_export("run", &[Value::I32(1)]).unwrap(),
            Some(Value::I32(11))
        );
        assert_eq!(
            vm.invoke_export("run", &[Value::I32(0)]).unwrap(),
            Some(Value::I32(22))
        );
    }

    #[test]
    fn branch_exits_block_with_result_value() {
        let bytes = module_with_body(
            0,
            1,
            &[
                0x02, 0x7f, // block (result i32)
                0x41, 0x2a, // i32.const 42
                0x0c, 0x00, // br 0
                0x41, 0x01, // dead code
                0x0b, // end block
                0x0b, // end function
            ],
        );
        assert_eq!(
            instance(&bytes).invoke_export("run", &[]).unwrap(),
            Some(Value::I32(42))
        );
    }

    #[test]
    fn branch_depth_can_exit_an_outer_block() {
        let bytes = module_with_body(
            0,
            1,
            &[
                0x02, 0x7f, // outer block (result i32)
                0x02, 0x40, // inner block
                0x41, 0x2a, // branch result = 42
                0x0c, 0x01, // br 1 -> outer block
                0x0b, // end inner
                0x41, 0x07, // statically valid fallthrough result; skipped at runtime
                0x0b, // end outer
                0x0b, // end function
            ],
        );
        assert_eq!(
            instance(&bytes).invoke_export("run", &[]).unwrap(),
            Some(Value::I32(42))
        );
    }

    #[test]
    fn loop_branch_restarts_loop_header() {
        let bytes = module_with_body(
            1,
            1,
            &[
                0x03, 0x40, // loop
                0x20, 0x00, // local.get 0
                0x41, 0x01, // i32.const 1
                0x6b, // i32.sub
                0x22, 0x00, // local.tee 0
                0x0d, 0x00, // br_if 0
                0x0b, // end loop
                0x20, 0x00, // local.get 0
                0x0b, // end function
            ],
        );
        assert_eq!(
            instance(&bytes)
                .invoke_export("run", &[Value::I32(3)])
                .unwrap(),
            Some(Value::I32(0))
        );
    }

    #[test]
    fn return_exits_nested_control_immediately() {
        let bytes = module_with_body(
            0,
            1,
            &[
                0x02, 0x40, // block
                0x41, 0x2a, // i32.const 42
                0x0f, // return
                0x0b, // end block
                0x41, 0x07, // dead function fallthrough
                0x0b, // end function
            ],
        );
        assert_eq!(
            instance(&bytes).invoke_export("run", &[]).unwrap(),
            Some(Value::I32(42))
        );
    }

    #[test]
    fn unsupported_opcode_is_rejected_before_execution() {
        let bytes = module_with_body(0, 1, &[0x01, 0x0b]);
        let module = parse_module(&bytes).expect("parse test module");
        let error = Instance::new(module).expect_err("unsupported opcode must fail validation");
        assert!(matches!(
            error,
            RuntimeError::Validation(ValidationError::UnsupportedOpcode { opcode: 0x01, .. })
        ));
    }
}
