//! Stack interpreter for the Phase-1 WebAssembly subset.

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
            Self::FunctionOutOfBounds(index) => write!(f, "function index {index} is out of bounds"),
            Self::UnsupportedType(ty) => write!(f, "runtime does not yet execute type {ty:?}"),
            Self::WrongArgumentCount { expected, actual } => {
                write!(f, "expected {expected} arguments, got {actual}")
            }
            Self::LocalOutOfBounds(index) => write!(f, "local index {index} is out of bounds"),
            Self::StackUnderflow => write!(f, "operand stack underflow"),
            Self::UnsupportedOpcode(opcode) => write!(f, "unsupported opcode 0x{opcode:02x}"),
            Self::ResultArityMismatch { expected, actual } => {
                write!(f, "expected {expected} result values, stack contains {actual}")
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

pub struct Instance {
    module: Module,
}

impl Instance {
    pub fn new(module: Module) -> Result<Self, RuntimeError> {
        validate(&module)?;
        Ok(Self { module })
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
            .ok_or(RuntimeError::FunctionOutOfBounds(function_index))? as usize;
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

        while pc < code.len() {
            let opcode = code[pc];
            pc += 1;
            match opcode {
                0x0b | 0x0f => break, // end / return
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

    fn module_with_body(params: u8, body: &[u8]) -> Vec<u8> {
        let type_payload = [
            0x01, 0x60, params, // one function type + parameter count
        ];
        let mut type_section = type_payload.to_vec();
        type_section.extend(std::iter::repeat(0x7f).take(params as usize));
        type_section.extend([0x01, 0x7f]); // one i32 result

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
        Instance::new(parse_module(bytes).expect("parse test module")).expect("validate test module")
    }

    #[test]
    fn executes_i32_add() {
        let bytes = module_with_body(2, &[0x20, 0x00, 0x20, 0x01, 0x6a, 0x0b]);
        let result = instance(&bytes)
            .invoke_export("run", &[Value::I32(20), Value::I32(22)])
            .expect("execution succeeds");
        assert_eq!(result, Some(Value::I32(42)));
    }

    #[test]
    fn integer_arithmetic_wraps_like_wasm() {
        let bytes = module_with_body(1, &[0x20, 0x00, 0x41, 0x01, 0x6a, 0x0b]);
        let result = instance(&bytes)
            .invoke_export("run", &[Value::I32(i32::MAX)])
            .expect("execution succeeds");
        assert_eq!(result, Some(Value::I32(i32::MIN)));
    }

    #[test]
    fn unsupported_opcode_fails_closed() {
        let bytes = module_with_body(0, &[0x01, 0x0b]);
        let error = instance(&bytes)
            .invoke_export("run", &[])
            .expect_err("nop is not in the Phase-1 subset");
        assert!(matches!(error, RuntimeError::UnsupportedOpcode(0x01)));
    }
}
