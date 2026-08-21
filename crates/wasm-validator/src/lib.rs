//! Cross-section and instruction-stream validation for the Phase-1 subset.

use std::{collections::HashSet, fmt};
use wasm_parser::{decode_i32, decode_u32, ExportKind, Module};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    FunctionCodeLengthMismatch { functions: usize, bodies: usize },
    TypeIndexOutOfBounds { function: usize, type_index: u32 },
    FunctionExportOutOfBounds { name: String, function_index: u32 },
    UnsupportedExportKind { name: String },
    DuplicateExportName(String),
    LocalCountOverflow { function: usize },
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
    MalformedImmediate { function: usize, offset: usize },
    UnexpectedEnd { function: usize, offset: usize },
    MissingFunctionEnd { function: usize },
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
                write!(f, "export {name:?} is not a function in the Phase-1 runtime")
            }
            Self::DuplicateExportName(name) => write!(f, "duplicate export name {name:?}"),
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
            Self::MalformedImmediate { function, offset } => write!(
                f,
                "function {function} has a malformed instruction immediate at byte {offset}"
            ),
            Self::UnexpectedEnd { function, offset } => write!(
                f,
                "function {function} has an unexpected end opcode at byte {offset}"
            ),
            Self::MissingFunctionEnd { function } => {
                write!(f, "function {function} is missing its final end opcode")
            }
        }
    }
}

impl std::error::Error for ValidationError {}

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

        let mut local_count = module.types[type_index as usize].params.len();
        for &(count, _) in &module.code[function].locals {
            local_count = local_count
                .checked_add(count as usize)
                .ok_or(ValidationError::LocalCountOverflow { function })?;
        }
        validate_code(module, function, local_count)?;
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

fn validate_code(
    module: &Module,
    function: usize,
    local_count: usize,
) -> Result<(), ValidationError> {
    let code = &module.code[function].code;
    if code.last().copied() != Some(0x0b) {
        return Err(ValidationError::MissingFunctionEnd { function });
    }

    let mut pc = 0usize;
    while pc < code.len() {
        let offset = pc;
        let opcode = code[pc];
        pc += 1;

        match opcode {
            0x0b => {
                if pc != code.len() {
                    return Err(ValidationError::UnexpectedEnd { function, offset });
                }
            }
            0x0f | 0x6a | 0x6b | 0x6c => {}
            0x20..=0x22 => {
                let local_index = read_u32_immediate(code, &mut pc, function, offset)?;
                if local_index as usize >= local_count {
                    return Err(ValidationError::LocalIndexOutOfBounds {
                        function,
                        offset,
                        local_index,
                    });
                }
            }
            0x41 => {
                let (_, used) = decode_i32(&code[pc..]).map_err(|_| {
                    ValidationError::MalformedImmediate { function, offset }
                })?;
                pc += used;
            }
            0x10 => {
                let target = read_u32_immediate(code, &mut pc, function, offset)?;
                if target as usize >= module.function_type_indices.len() {
                    return Err(ValidationError::CallTargetOutOfBounds {
                        function,
                        offset,
                        target,
                    });
                }
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

    Ok(())
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
    use wasm_parser::{Export, FuncType, FunctionBody, ValueType};

    fn valid_module() -> Module {
        Module {
            types: vec![FuncType {
                params: vec![ValueType::I32],
                results: vec![ValueType::I32],
            }],
            function_type_indices: vec![0],
            exports: vec![Export {
                name: "id".into(),
                kind: ExportKind::Function,
                index: 0,
            }],
            code: vec![FunctionBody {
                locals: vec![],
                code: vec![0x20, 0x00, 0x0b],
            }],
        }
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
    fn rejects_unsupported_opcode_even_after_return() {
        let mut module = valid_module();
        module.code[0].code = vec![0x20, 0x00, 0x0f, 0x01, 0x0b];
        assert_eq!(
            validate(&module),
            Err(ValidationError::UnsupportedOpcode {
                function: 0,
                offset: 3,
                opcode: 0x01,
            })
        );
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
    fn rejects_premature_end() {
        let mut module = valid_module();
        module.code[0].code = vec![0x0b, 0x0b];
        assert_eq!(
            validate(&module),
            Err(ValidationError::UnexpectedEnd {
                function: 0,
                offset: 0,
            })
        );
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
