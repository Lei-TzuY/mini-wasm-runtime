//! Cross-section structural validation for the Phase-1 WebAssembly subset.

use std::{collections::HashSet, fmt};
use wasm_parser::{ExportKind, Module};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    FunctionCodeLengthMismatch { functions: usize, bodies: usize },
    TypeIndexOutOfBounds { function: usize, type_index: u32 },
    FunctionExportOutOfBounds { name: String, function_index: u32 },
    UnsupportedExportKind { name: String },
    DuplicateExportName(String),
    LocalCountOverflow { function: usize },
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

        let mut total = 0usize;
        for &(count, _) in &module.code[function].locals {
            total = total
                .checked_add(count as usize)
                .ok_or(ValidationError::LocalCountOverflow { function })?;
        }
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
}
