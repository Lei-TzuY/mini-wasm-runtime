//! Typed validation for the executable WebAssembly subset.
//!
//! Phase 2 keeps the value domain intentionally small (i32 only), but moves
//! from structural opcode scanning to a real operand/control-stack validator.

use std::{collections::HashSet, fmt};
use wasm_parser::{decode_i32, decode_u32, ExportKind, Module, ValueType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    FunctionCodeLengthMismatch {
        functions: usize,
        bodies: usize,
    },
    TypeIndexOutOfBounds {
        function: usize,
        type_index: u32,
    },
    FunctionExportOutOfBounds {
        name: String,
        function_index: u32,
    },
    UnsupportedExportKind {
        name: String,
    },
    DuplicateExportName(String),
    UnsupportedResultArity {
        function: usize,
        results: usize,
    },
    UnsupportedValueType {
        function: usize,
        value_type: ValueType,
    },
    LocalCountOverflow {
        function: usize,
    },
    LocalIndexOutOfBounds {
        function: usize,
        offset: usize,
        local_index: u32,
    },
    CallTargetOutOfBounds {
        function: usize,
        offset: usize,
        target: u32,
    },
    UnsupportedOpcode {
        function: usize,
        offset: usize,
        opcode: u8,
    },
    UnsupportedBlockType {
        function: usize,
        offset: usize,
        block_type: u8,
    },
    MalformedImmediate {
        function: usize,
        offset: usize,
    },
    OperandStackUnderflow {
        function: usize,
        offset: usize,
    },
    StackHeightMismatch {
        function: usize,
        offset: usize,
        expected: usize,
        actual: usize,
    },
    BranchDepthOutOfBounds {
        function: usize,
        offset: usize,
        depth: u32,
    },
    UnexpectedElse {
        function: usize,
        offset: usize,
    },
    DuplicateElse {
        function: usize,
        offset: usize,
    },
    MissingElseForResult {
        function: usize,
        offset: usize,
    },
    UnexpectedEnd {
        function: usize,
        offset: usize,
    },
    MissingFunctionEnd {
        function: usize,
    },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FunctionCodeLengthMismatch { functions, bodies } => write!(
                f,
                "function section declares {functions} functions but code section has {bodies} bodies"
            ),
            Self::TypeIndexOutOfBounds {
                function,
                type_index,
            } => write!(
                f,
                "function {function} refers to missing type index {type_index}"
            ),
            Self::FunctionExportOutOfBounds {
                name,
                function_index,
            } => write!(
                f,
                "export {name:?} refers to missing function index {function_index}"
            ),
            Self::UnsupportedExportKind { name } => {
                write!(f, "export {name:?} is not a function in this runtime")
            }
            Self::DuplicateExportName(name) => write!(f, "duplicate export name {name:?}"),
            Self::UnsupportedResultArity { function, results } => write!(
                f,
                "function {function} has {results} results; this runtime supports at most one"
            ),
            Self::UnsupportedValueType {
                function,
                value_type,
            } => write!(
                f,
                "function {function} uses {value_type:?}; execution is currently i32-only"
            ),
            Self::LocalCountOverflow { function } => {
                write!(f, "local declaration count overflows usize in function {function}")
            }
            Self::LocalIndexOutOfBounds {
                function,
                offset,
                local_index,
            } => write!(
                f,
                "function {function} local instruction at byte {offset} refers to missing local {local_index}"
            ),
            Self::CallTargetOutOfBounds {
                function,
                offset,
                target,
            } => write!(
                f,
                "function {function} call at byte {offset} refers to missing function {target}"
            ),
            Self::UnsupportedOpcode {
                function,
                offset,
                opcode,
            } => write!(
                f,
                "function {function} uses unsupported opcode 0x{opcode:02x} at byte {offset}"
            ),
            Self::UnsupportedBlockType {
                function,
                offset,
                block_type,
            } => write!(
                f,
                "function {function} uses unsupported block type 0x{block_type:02x} at byte {offset}"
            ),
            Self::MalformedImmediate { function, offset } => write!(
                f,
                "function {function} has a malformed instruction immediate at byte {offset}"
            ),
            Self::OperandStackUnderflow { function, offset } => write!(
                f,
                "function {function} operand stack underflows at byte {offset}"
            ),
            Self::StackHeightMismatch {
                function,
                offset,
                expected,
                actual,
            } => write!(
                f,
                "function {function} control frame ending at byte {offset} expects stack height {expected}, got {actual}"
            ),
            Self::BranchDepthOutOfBounds {
                function,
                offset,
                depth,
            } => write!(
                f,
                "function {function} branch at byte {offset} refers to missing label depth {depth}"
            ),
            Self::UnexpectedElse { function, offset } => write!(
                f,
                "function {function} has else without a matching if at byte {offset}"
            ),
            Self::DuplicateElse { function, offset } => write!(
                f,
                "function {function} has a duplicate else at byte {offset}"
            ),
            Self::MissingElseForResult { function, offset } => write!(
                f,
                "function {function} has a result-producing if without else ending at byte {offset}"
            ),
            Self::UnexpectedEnd { function, offset } => write!(
                f,
                "function {function} has bytes after its final end opcode at byte {offset}"
            ),
            Self::MissingFunctionEnd { function } => {
                write!(f, "function {function} is missing its final end opcode")
            }
        }
    }
}

impl std::error::Error for ValidationError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ControlKind {
    Function,
    Block,
    Loop,
    If,
}

#[derive(Debug, Clone, Copy)]
struct ControlFrame {
    kind: ControlKind,
    height: usize,
    end_arity: usize,
    label_arity: usize,
    unreachable: bool,
    seen_else: bool,
}

pub fn validate(module: &Module) -> Result<(), ValidationError> {
    if module.function_type_indices.len() != module.code.len() {
        return Err(ValidationError::FunctionCodeLengthMismatch {
            functions: module.function_type_indices.len(),
            bodies: module.code.len(),
        });
    }

    for (function, &type_index) in module.function_type_indices.iter().enumerate() {
        if type_index as usize >= module.types.len() {
            return Err(ValidationError::TypeIndexOutOfBounds {
                function,
                type_index,
            });
        }
    }

    for (function, &type_index) in module.function_type_indices.iter().enumerate() {
        let function_type = &module.types[type_index as usize];
        if function_type.results.len() > 1 {
            return Err(ValidationError::UnsupportedResultArity {
                function,
                results: function_type.results.len(),
            });
        }
        for &value_type in function_type
            .params
            .iter()
            .chain(function_type.results.iter())
        {
            ensure_i32(function, value_type)?;
        }

        let mut local_count = function_type.params.len();
        for &(count, value_type) in &module.code[function].locals {
            ensure_i32(function, value_type)?;
            local_count = local_count
                .checked_add(count as usize)
                .ok_or(ValidationError::LocalCountOverflow { function })?;
        }

        validate_code(module, function, local_count, function_type.results.len())?;
    }

    let mut names = HashSet::new();
    for export in &module.exports {
        if !names.insert(export.name.as_str()) {
            return Err(ValidationError::DuplicateExportName(export.name.clone()));
        }
        if export.kind != ExportKind::Function {
            return Err(ValidationError::UnsupportedExportKind {
                name: export.name.clone(),
            });
        }
        if export.index as usize >= module.function_type_indices.len() {
            return Err(ValidationError::FunctionExportOutOfBounds {
                name: export.name.clone(),
                function_index: export.index,
            });
        }
    }

    Ok(())
}

fn ensure_i32(function: usize, value_type: ValueType) -> Result<(), ValidationError> {
    if value_type == ValueType::I32 {
        Ok(())
    } else {
        Err(ValidationError::UnsupportedValueType {
            function,
            value_type,
        })
    }
}

fn validate_code(
    module: &Module,
    function: usize,
    local_count: usize,
    function_result_arity: usize,
) -> Result<(), ValidationError> {
    let code = &module.code[function].code;
    if code.last().copied() != Some(0x0b) {
        return Err(ValidationError::MissingFunctionEnd { function });
    }

    let mut stack_height = 0usize;
    let mut controls = vec![ControlFrame {
        kind: ControlKind::Function,
        height: 0,
        end_arity: function_result_arity,
        label_arity: function_result_arity,
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
                let end_arity = read_block_arity(code, &mut pc, function, offset)?;
                let kind = if opcode == 0x02 {
                    ControlKind::Block
                } else {
                    ControlKind::Loop
                };
                controls.push(ControlFrame {
                    kind,
                    height: stack_height,
                    end_arity,
                    label_arity: if kind == ControlKind::Loop {
                        0
                    } else {
                        end_arity
                    },
                    unreachable: false,
                    seen_else: false,
                });
            }
            0x04 => {
                let end_arity = read_block_arity(code, &mut pc, function, offset)?;
                pop_values(&mut stack_height, &controls, 1, function, offset)?;
                controls.push(ControlFrame {
                    kind: ControlKind::If,
                    height: stack_height,
                    end_arity,
                    label_arity: end_arity,
                    unreachable: false,
                    seen_else: false,
                });
            }
            0x05 => transition_to_else(&mut stack_height, &mut controls, function, offset)?,
            0x0b => {
                let frame = *controls
                    .last()
                    .ok_or(ValidationError::UnexpectedEnd { function, offset })?;
                if frame.kind == ControlKind::If && frame.end_arity > 0 && !frame.seen_else {
                    return Err(ValidationError::MissingElseForResult { function, offset });
                }

                finish_frame(&mut stack_height, &frame, function, offset)?;
                controls.pop();

                if frame.kind == ControlKind::Function {
                    if pc != code.len() {
                        return Err(ValidationError::UnexpectedEnd { function, offset });
                    }
                } else {
                    stack_height = frame.height + frame.end_arity;
                }
            }
            0x0c => {
                let depth = read_u32_immediate(code, &mut pc, function, offset)?;
                let label_arity = label_arity(&controls, depth, function, offset)?;
                require_values(stack_height, &controls, label_arity, function, offset)?;
                mark_unreachable(&mut stack_height, &mut controls);
            }
            0x0d => {
                let depth = read_u32_immediate(code, &mut pc, function, offset)?;
                pop_values(&mut stack_height, &controls, 1, function, offset)?;
                let label_arity = label_arity(&controls, depth, function, offset)?;
                require_values(stack_height, &controls, label_arity, function, offset)?;
            }
            0x0f => {
                let result_arity = controls.first().map(|frame| frame.label_arity).unwrap_or(0);
                require_values(stack_height, &controls, result_arity, function, offset)?;
                mark_unreachable(&mut stack_height, &mut controls);
            }
            0x10 => {
                let target = read_u32_immediate(code, &mut pc, function, offset)?;
                let target_function = target as usize;
                if target_function >= module.function_type_indices.len() {
                    return Err(ValidationError::CallTargetOutOfBounds {
                        function,
                        offset,
                        target,
                    });
                }
                let target_type = module.function_type_indices[target_function] as usize;
                let ty = &module.types[target_type];
                pop_values(
                    &mut stack_height,
                    &controls,
                    ty.params.len(),
                    function,
                    offset,
                )?;
                stack_height += ty.results.len();
            }
            0x20 => {
                read_local_index(code, &mut pc, function, offset, local_count)?;
                stack_height += 1;
            }
            0x21 => {
                read_local_index(code, &mut pc, function, offset, local_count)?;
                pop_values(&mut stack_height, &controls, 1, function, offset)?;
            }
            0x22 => {
                read_local_index(code, &mut pc, function, offset, local_count)?;
                require_values(stack_height, &controls, 1, function, offset)?;
            }
            0x41 => {
                let (_, used) = decode_i32(&code[pc..])
                    .map_err(|_| ValidationError::MalformedImmediate { function, offset })?;
                pc += used;
                stack_height += 1;
            }
            0x6a..=0x6c => {
                pop_values(&mut stack_height, &controls, 2, function, offset)?;
                stack_height += 1;
            }
            opcode => {
                return Err(ValidationError::UnsupportedOpcode {
                    function,
                    offset,
                    opcode,
                });
            }
        }
    }

    if !controls.is_empty() {
        return Err(ValidationError::MissingFunctionEnd { function });
    }
    Ok(())
}

fn transition_to_else(
    stack_height: &mut usize,
    controls: &mut [ControlFrame],
    function: usize,
    offset: usize,
) -> Result<(), ValidationError> {
    let frame = *controls
        .last()
        .ok_or(ValidationError::UnexpectedElse { function, offset })?;
    if frame.kind != ControlKind::If {
        return Err(ValidationError::UnexpectedElse { function, offset });
    }
    if frame.seen_else {
        return Err(ValidationError::DuplicateElse { function, offset });
    }

    finish_frame(stack_height, &frame, function, offset)?;
    *stack_height = frame.height;
    let current = controls
        .last_mut()
        .ok_or(ValidationError::UnexpectedElse { function, offset })?;
    current.unreachable = false;
    current.seen_else = true;
    Ok(())
}

fn finish_frame(
    stack_height: &mut usize,
    frame: &ControlFrame,
    function: usize,
    offset: usize,
) -> Result<(), ValidationError> {
    if frame.unreachable {
        *stack_height = frame.height;
        return Ok(());
    }

    let expected = frame.height + frame.end_arity;
    if *stack_height != expected {
        return Err(ValidationError::StackHeightMismatch {
            function,
            offset,
            expected,
            actual: *stack_height,
        });
    }
    Ok(())
}

fn pop_values(
    stack_height: &mut usize,
    controls: &[ControlFrame],
    count: usize,
    function: usize,
    offset: usize,
) -> Result<(), ValidationError> {
    let frame = controls
        .last()
        .ok_or(ValidationError::OperandStackUnderflow { function, offset })?;
    let available = stack_height.saturating_sub(frame.height);
    if available < count {
        if frame.unreachable {
            *stack_height = frame.height;
            return Ok(());
        }
        return Err(ValidationError::OperandStackUnderflow { function, offset });
    }
    *stack_height -= count;
    Ok(())
}

fn require_values(
    stack_height: usize,
    controls: &[ControlFrame],
    count: usize,
    function: usize,
    offset: usize,
) -> Result<(), ValidationError> {
    let frame = controls
        .last()
        .ok_or(ValidationError::OperandStackUnderflow { function, offset })?;
    let available = stack_height.saturating_sub(frame.height);
    if available < count && !frame.unreachable {
        return Err(ValidationError::OperandStackUnderflow { function, offset });
    }
    Ok(())
}

fn mark_unreachable(stack_height: &mut usize, controls: &mut [ControlFrame]) {
    if let Some(frame) = controls.last_mut() {
        *stack_height = frame.height;
        frame.unreachable = true;
    }
}

fn label_arity(
    controls: &[ControlFrame],
    depth: u32,
    function: usize,
    offset: usize,
) -> Result<usize, ValidationError> {
    let depth = depth as usize;
    let index =
        controls
            .len()
            .checked_sub(depth + 1)
            .ok_or(ValidationError::BranchDepthOutOfBounds {
                function,
                offset,
                depth: depth as u32,
            })?;
    Ok(controls[index].label_arity)
}

fn read_block_arity(
    code: &[u8],
    pc: &mut usize,
    function: usize,
    offset: usize,
) -> Result<usize, ValidationError> {
    let block_type = *code
        .get(*pc)
        .ok_or(ValidationError::MalformedImmediate { function, offset })?;
    *pc += 1;
    match block_type {
        0x40 => Ok(0),
        0x7f => Ok(1),
        _ => Err(ValidationError::UnsupportedBlockType {
            function,
            offset,
            block_type,
        }),
    }
}

fn read_local_index(
    code: &[u8],
    pc: &mut usize,
    function: usize,
    offset: usize,
    local_count: usize,
) -> Result<u32, ValidationError> {
    let local_index = read_u32_immediate(code, pc, function, offset)?;
    if local_index as usize >= local_count {
        return Err(ValidationError::LocalIndexOutOfBounds {
            function,
            offset,
            local_index,
        });
    }
    Ok(local_index)
}

fn read_u32_immediate(
    code: &[u8],
    pc: &mut usize,
    function: usize,
    offset: usize,
) -> Result<u32, ValidationError> {
    let (value, used) = decode_u32(&code[*pc..])
        .map_err(|_| ValidationError::MalformedImmediate { function, offset })?;
    *pc += used;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_parser::{Export, FuncType, FunctionBody};

    fn module_with_code(params: usize, results: usize, code: Vec<u8>) -> Module {
        Module {
            types: vec![FuncType {
                params: vec![ValueType::I32; params],
                results: vec![ValueType::I32; results],
            }],
            function_type_indices: vec![0],
            exports: vec![Export {
                name: "run".into(),
                kind: ExportKind::Function,
                index: 0,
            }],
            code: vec![FunctionBody {
                locals: vec![],
                code,
            }],
        }
    }

    fn valid_module() -> Module {
        module_with_code(1, 1, vec![0x20, 0x00, 0x0b])
    }

    #[test]
    fn accepts_structurally_valid_module() {
        assert_eq!(validate(&valid_module()), Ok(()));
    }

    #[test]
    fn catches_function_code_mismatch() {
        let mut module = valid_module();
        module.code.clear();
        assert!(matches!(
            validate(&module),
            Err(ValidationError::FunctionCodeLengthMismatch { .. })
        ));
    }

    #[test]
    fn catches_bad_type_index() {
        let mut module = valid_module();
        module.function_type_indices[0] = 9;
        assert!(matches!(
            validate(&module),
            Err(ValidationError::TypeIndexOutOfBounds { .. })
        ));
    }

    #[test]
    fn rejects_multi_value_results() {
        let mut module = valid_module();
        module.types[0].results.push(ValueType::I32);
        assert_eq!(
            validate(&module),
            Err(ValidationError::UnsupportedResultArity {
                function: 0,
                results: 2,
            })
        );
    }

    #[test]
    fn rejects_non_i32_execution_types() {
        let mut module = valid_module();
        module.types[0].params[0] = ValueType::I64;
        assert_eq!(
            validate(&module),
            Err(ValidationError::UnsupportedValueType {
                function: 0,
                value_type: ValueType::I64,
            })
        );
    }

    #[test]
    fn accepts_typed_if_else_result() {
        let module = module_with_code(
            1,
            1,
            vec![
                0x20, 0x00, // local.get 0
                0x04, 0x7f, // if (result i32)
                0x41, 0x01, // i32.const 1
                0x05, // else
                0x41, 0x02, // i32.const 2
                0x0b, // end if
                0x0b, // end function
            ],
        );
        assert_eq!(validate(&module), Ok(()));
    }

    #[test]
    fn accepts_block_branch_with_result() {
        let module = module_with_code(
            0,
            1,
            vec![
                0x02, 0x7f, // block (result i32)
                0x41, 0x2a, // i32.const 42
                0x0c, 0x00, // br 0
                0x0b, // end block
                0x0b, // end function
            ],
        );
        assert_eq!(validate(&module), Ok(()));
    }

    #[test]
    fn accepts_loop_and_conditional_branch() {
        let module = module_with_code(
            1,
            1,
            vec![
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
        assert_eq!(validate(&module), Ok(()));
    }

    #[test]
    fn unreachable_code_is_stack_polymorphic_but_still_opcode_checked() {
        let valid = module_with_code(
            1,
            1,
            vec![
                0x20, 0x00, // result
                0x0f, // return
                0x6a, // unreachable i32.add: stack-polymorphic
                0x0b,
            ],
        );
        assert_eq!(validate(&valid), Ok(()));

        let invalid = module_with_code(1, 1, vec![0x20, 0x00, 0x0f, 0x01, 0x0b]);
        assert!(matches!(
            validate(&invalid),
            Err(ValidationError::UnsupportedOpcode { opcode: 0x01, .. })
        ));
    }

    #[test]
    fn rejects_operand_stack_underflow() {
        let module = module_with_code(0, 1, vec![0x6a, 0x0b]);
        assert!(matches!(
            validate(&module),
            Err(ValidationError::OperandStackUnderflow { .. })
        ));
    }

    #[test]
    fn rejects_control_result_stack_mismatch() {
        let module = module_with_code(0, 1, vec![0x02, 0x7f, 0x0b, 0x0b]);
        assert!(matches!(
            validate(&module),
            Err(ValidationError::StackHeightMismatch { .. })
        ));
    }

    #[test]
    fn rejects_result_if_without_else() {
        let module = module_with_code(1, 1, vec![0x20, 0x00, 0x04, 0x7f, 0x41, 0x01, 0x0b, 0x0b]);
        assert!(matches!(
            validate(&module),
            Err(ValidationError::MissingElseForResult { .. })
        ));
    }

    #[test]
    fn rejects_branch_depth_out_of_bounds() {
        let module = module_with_code(0, 0, vec![0x02, 0x40, 0x0c, 0x02, 0x0b, 0x0b]);
        assert!(matches!(
            validate(&module),
            Err(ValidationError::BranchDepthOutOfBounds { depth: 2, .. })
        ));
    }

    #[test]
    fn rejects_unexpected_else() {
        let module = module_with_code(0, 0, vec![0x05, 0x0b]);
        assert!(matches!(
            validate(&module),
            Err(ValidationError::UnexpectedElse { .. })
        ));
    }

    #[test]
    fn rejects_unsupported_block_type() {
        let module = module_with_code(0, 0, vec![0x02, 0x7e, 0x0b, 0x0b]);
        assert!(matches!(
            validate(&module),
            Err(ValidationError::UnsupportedBlockType {
                block_type: 0x7e,
                ..
            })
        ));
    }

    #[test]
    fn rejects_out_of_bounds_local() {
        let mut module = valid_module();
        module.code[0].code = vec![0x20, 0x01, 0x0b];
        assert!(matches!(
            validate(&module),
            Err(ValidationError::LocalIndexOutOfBounds { .. })
        ));
    }

    #[test]
    fn rejects_out_of_bounds_call() {
        let mut module = valid_module();
        module.code[0].code = vec![0x10, 0x01, 0x0b];
        assert!(matches!(
            validate(&module),
            Err(ValidationError::CallTargetOutOfBounds { .. })
        ));
    }

    #[test]
    fn rejects_malformed_immediate() {
        let mut module = valid_module();
        module.code[0].code = vec![0x20, 0x80, 0x80, 0x80, 0x80, 0x80, 0x0b];
        assert!(matches!(
            validate(&module),
            Err(ValidationError::MalformedImmediate { .. })
        ));
    }
}
