use super::{function_type, ValidationError};
use wasm_parser::Module;

pub(super) fn validate_phase5(module: &Module) -> Result<(), ValidationError> {
    if module.tables.len() > 1 {
        return Err(ValidationError::UnsupportedTableCount {
            count: module.tables.len(),
        });
    }
    for (table, table_type) in module.tables.iter().enumerate() {
        if let Some(max) = table_type.limits.max {
            if table_type.limits.min > max {
                return Err(ValidationError::InvalidTableLimits {
                    table,
                    min: table_type.limits.min,
                    max,
                });
            }
        }
    }

    if let Some(start) = module.start {
        let Some(ty) = function_type(module, start) else {
            return Err(ValidationError::StartFunctionOutOfBounds {
                function_index: start,
            });
        };
        if !ty.params.is_empty() || !ty.results.is_empty() {
            return Err(ValidationError::InvalidStartSignature {
                function_index: start,
            });
        }
    }

    let total_functions = module.imports.len() + module.function_type_indices.len();
    for (segment, element) in module.elements.iter().enumerate() {
        if element.table_index as usize >= module.tables.len() {
            return Err(ValidationError::ElementTableOutOfBounds {
                segment,
                table_index: element.table_index,
            });
        }
        for &function_index in &element.function_indices {
            if function_index as usize >= total_functions {
                return Err(ValidationError::ElementFunctionOutOfBounds {
                    segment,
                    function_index,
                });
            }
        }
    }
    Ok(())
}

pub(super) fn validate_global_export(module: &Module, index: u32) -> bool {
    (index as usize) < module.globals.len()
}
