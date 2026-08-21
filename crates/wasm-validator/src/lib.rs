//! Typed validation for the executable WebAssembly subset.
//!
//! Phase 5B validates every reachable operand as an explicit MVP numeric type.
//! Function imports and defined code may use i32/i64/f32/f64 with at most one result.

use std::{collections::HashSet, fmt};
use wasm_parser::{decode_u32, DataMode, ExportKind, FuncType, ImportDesc, Module, ValueType};

mod phase5;
mod typed;

pub const MAX_MEMORY_PAGES: u32 = 65_536;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    FunctionCodeLengthMismatch {
        functions: usize,
        bodies: usize,
    },
    ImportTypeIndexOutOfBounds {
        import: usize,
        type_index: u32,
    },
    UnsupportedImportResultArity {
        import: usize,
        results: usize,
    },
    TypeIndexOutOfBounds {
        function: usize,
        type_index: u32,
    },
    FunctionExportOutOfBounds {
        name: String,
        function_index: u32,
    },
    MemoryExportOutOfBounds {
        name: String,
        memory_index: u32,
    },
    TableExportOutOfBounds {
        name: String,
        table_index: u32,
    },
    GlobalExportOutOfBounds {
        name: String,
        global_index: u32,
    },
    UnsupportedTableCount {
        count: usize,
    },
    InvalidTableLimits {
        table: usize,
        min: u32,
        max: u32,
    },
    UnsupportedGlobalValueType {
        global: usize,
        value_type: ValueType,
    },
    StartFunctionOutOfBounds {
        function_index: u32,
    },
    InvalidStartSignature {
        function_index: u32,
    },
    ElementTableOutOfBounds {
        segment: usize,
        table_index: u32,
    },
    ElementFunctionOutOfBounds {
        segment: usize,
        function_index: u32,
    },
    GlobalIndexOutOfBounds {
        function: usize,
        offset: usize,
        global_index: u32,
    },
    ImmutableGlobalSet {
        function: usize,
        offset: usize,
        global_index: u32,
    },
    TableIndexOutOfBounds {
        function: usize,
        offset: usize,
        table_index: u32,
    },
    IndirectTypeIndexOutOfBounds {
        function: usize,
        offset: usize,
        type_index: u32,
    },
    UnsupportedIndirectResultArity {
        function: usize,
        offset: usize,
        results: usize,
    },
    UnsupportedIndirectValueType {
        function: usize,
        offset: usize,
        value_type: ValueType,
    },
    UnsupportedExportKind {
        name: String,
    },
    DuplicateExportName(String),
    UnsupportedMemoryCount {
        count: usize,
    },
    InvalidMemoryLimits {
        memory: usize,
        min: u32,
        max: u32,
    },
    MemoryPageLimitExceeded {
        memory: usize,
        pages: u32,
    },
    DataMemoryOutOfBounds {
        segment: usize,
        memory_index: u32,
    },
    MemoryInstructionWithoutMemory {
        function: usize,
        offset: usize,
    },
    MemoryIndexOutOfBounds {
        function: usize,
        offset: usize,
        memory_index: u32,
    },
    InvalidMemoryAlignment {
        function: usize,
        offset: usize,
        alignment: u32,
        maximum: u32,
    },
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
    BlockTypeIndexOutOfBounds {
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
    TypeMismatch {
        function: usize,
        offset: usize,
        expected: ValueType,
        actual: ValueType,
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
            Self::ImportTypeIndexOutOfBounds { import, type_index } => write!(
                f,
                "function import {import} refers to missing type index {type_index}"
            ),
            Self::UnsupportedImportResultArity { import, results } => write!(
                f,
                "function import {import} has {results} results; this runtime supports at most one"
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
            Self::MemoryExportOutOfBounds {
                name,
                memory_index,
            } => write!(
                f,
                "export {name:?} refers to missing memory index {memory_index}"
            ),
            Self::TableExportOutOfBounds { name, table_index } => write!(
                f,
                "export {name:?} refers to missing table index {table_index}"
            ),
            Self::GlobalExportOutOfBounds { name, global_index } => write!(
                f,
                "export {name:?} refers to missing global index {global_index}"
            ),
            Self::UnsupportedTableCount { count } => {
                write!(f, "this runtime supports at most one table, got {count}")
            }
            Self::InvalidTableLimits { table, min, max } => write!(
                f,
                "table {table} has invalid limits: minimum {min} exceeds maximum {max}"
            ),
            Self::UnsupportedGlobalValueType { global, value_type } => write!(
                f,
                "global {global} uses {value_type:?}; globals are currently i32-only"
            ),
            Self::StartFunctionOutOfBounds { function_index } => {
                write!(f, "start function index {function_index} is out of bounds")
            }
            Self::InvalidStartSignature { function_index } => write!(
                f,
                "start function {function_index} must have signature [] -> []"
            ),
            Self::ElementTableOutOfBounds { segment, table_index } => write!(
                f,
                "element segment {segment} refers to missing table index {table_index}"
            ),
            Self::ElementFunctionOutOfBounds { segment, function_index } => write!(
                f,
                "element segment {segment} refers to missing function index {function_index}"
            ),
            Self::GlobalIndexOutOfBounds { function, offset, global_index } => write!(
                f,
                "function {function} global instruction at byte {offset} refers to missing global {global_index}"
            ),
            Self::ImmutableGlobalSet { function, offset, global_index } => write!(
                f,
                "function {function} global.set at byte {offset} targets immutable global {global_index}"
            ),
            Self::TableIndexOutOfBounds { function, offset, table_index } => write!(
                f,
                "function {function} call_indirect at byte {offset} refers to missing table {table_index}"
            ),
            Self::IndirectTypeIndexOutOfBounds { function, offset, type_index } => write!(
                f,
                "function {function} call_indirect at byte {offset} refers to missing type {type_index}"
            ),
            Self::UnsupportedIndirectResultArity { function, offset, results } => write!(
                f,
                "function {function} call_indirect at byte {offset} uses a type with {results} results; at most one is supported"
            ),
            Self::UnsupportedIndirectValueType { function, offset, value_type } => write!(
                f,
                "function {function} call_indirect at byte {offset} uses unsupported {value_type:?}"
            ),
            Self::UnsupportedExportKind { name } => {
                write!(f, "export {name:?} has a kind not supported by this runtime")
            }
            Self::DuplicateExportName(name) => write!(f, "duplicate export name {name:?}"),
            Self::UnsupportedMemoryCount { count } => {
                write!(f, "this runtime supports at most one linear memory, got {count}")
            }
            Self::InvalidMemoryLimits { memory, min, max } => write!(
                f,
                "memory {memory} has invalid limits: minimum {min} exceeds maximum {max}"
            ),
            Self::MemoryPageLimitExceeded { memory, pages } => write!(
                f,
                "memory {memory} declares {pages} pages, exceeding the WebAssembly limit of {MAX_MEMORY_PAGES}"
            ),
            Self::DataMemoryOutOfBounds {
                segment,
                memory_index,
            } => write!(
                f,
                "data segment {segment} refers to missing memory index {memory_index}"
            ),
            Self::MemoryInstructionWithoutMemory { function, offset } => write!(
                f,
                "function {function} uses a memory instruction at byte {offset} but the module declares no memory"
            ),
            Self::MemoryIndexOutOfBounds {
                function,
                offset,
                memory_index,
            } => write!(
                f,
                "function {function} memory instruction at byte {offset} refers to missing memory {memory_index}"
            ),
            Self::InvalidMemoryAlignment {
                function,
                offset,
                alignment,
                maximum,
            } => write!(
                f,
                "function {function} memory instruction at byte {offset} uses alignment exponent {alignment}, maximum is {maximum}"
            ),
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
            Self::BlockTypeIndexOutOfBounds {
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
            Self::TypeMismatch {
                function,
                offset,
                expected,
                actual,
            } => write!(
                f,
                "function {function} expects {expected:?} at byte {offset}, got {actual:?}"
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

pub fn validate(module: &Module) -> Result<(), ValidationError> {
    validate_memories(module)?;
    validate_imports(module)?;
    phase5::validate_phase5(module)?;

    if module.function_type_indices.len() != module.code.len() {
        return Err(ValidationError::FunctionCodeLengthMismatch {
            functions: module.function_type_indices.len(),
            bodies: module.code.len(),
        });
    }

    for (defined, &type_index) in module.function_type_indices.iter().enumerate() {
        let function = module.function_import_count() + defined;
        if type_index as usize >= module.types.len() {
            return Err(ValidationError::TypeIndexOutOfBounds {
                function,
                type_index,
            });
        }
    }

    for (defined, &type_index) in module.function_type_indices.iter().enumerate() {
        let function = module.function_import_count() + defined;
        let function_type = &module.types[type_index as usize];
        if function_type.results.len() > 1 {
            return Err(ValidationError::UnsupportedResultArity {
                function,
                results: function_type.results.len(),
            });
        }
        let mut local_types = function_type.params.clone();
        for &(count, value_type) in &module.code[defined].locals {
            let new_len = local_types
                .len()
                .checked_add(count as usize)
                .ok_or(ValidationError::LocalCountOverflow { function })?;
            local_types.resize(new_len, value_type);
        }

        typed::validate_code(
            module,
            defined,
            function,
            &local_types,
            &function_type.results,
        )?;
    }

    let total_functions = module.function_count();
    let mut names = HashSet::new();
    for export in &module.exports {
        if !names.insert(export.name.as_str()) {
            return Err(ValidationError::DuplicateExportName(export.name.clone()));
        }
        match export.kind {
            ExportKind::Function => {
                if export.index as usize >= total_functions {
                    return Err(ValidationError::FunctionExportOutOfBounds {
                        name: export.name.clone(),
                        function_index: export.index,
                    });
                }
            }
            ExportKind::Memory => {
                if export.index as usize >= module.memory_count() {
                    return Err(ValidationError::MemoryExportOutOfBounds {
                        name: export.name.clone(),
                        memory_index: export.index,
                    });
                }
            }
            ExportKind::Table => {
                if export.index as usize >= module.table_count() {
                    return Err(ValidationError::TableExportOutOfBounds {
                        name: export.name.clone(),
                        table_index: export.index,
                    });
                }
            }
            ExportKind::Global => {
                if !phase5::validate_global_export(module, export.index) {
                    return Err(ValidationError::GlobalExportOutOfBounds {
                        name: export.name.clone(),
                        global_index: export.index,
                    });
                }
            }
        }
    }

    for (segment, data) in module.data.iter().enumerate() {
        let DataMode::Active { memory_index, .. } = data.mode else {
            continue;
        };
        if memory_index as usize >= module.memory_count() {
            return Err(ValidationError::DataMemoryOutOfBounds {
                segment,
                memory_index,
            });
        }
    }

    Ok(())
}

fn validate_imports(module: &Module) -> Result<(), ValidationError> {
    for (import, entry) in module.imports.iter().enumerate() {
        match entry.desc {
            ImportDesc::Function(type_index) => {
                let Some(function_type) = module.types.get(type_index as usize) else {
                    return Err(ValidationError::ImportTypeIndexOutOfBounds { import, type_index });
                };
                if function_type.results.len() > 1 {
                    return Err(ValidationError::UnsupportedImportResultArity {
                        import,
                        results: function_type.results.len(),
                    });
                }
            }
            ImportDesc::Table(table_type) => {
                if let Some(max) = table_type.limits.max {
                    if table_type.limits.min > max {
                        return Err(ValidationError::InvalidTableLimits {
                            table: import,
                            min: table_type.limits.min,
                            max,
                        });
                    }
                }
            }
            ImportDesc::Memory(memory_type) => {
                validate_memory_type(import, memory_type.limits.min, memory_type.limits.max)?;
            }
            ImportDesc::Global(_) => {}
        }
    }
    Ok(())
}

fn validate_memories(module: &Module) -> Result<(), ValidationError> {
    if module.memory_count() > 1 {
        return Err(ValidationError::UnsupportedMemoryCount {
            count: module.memory_count(),
        });
    }

    for memory in 0..module.memory_count() {
        let memory_type = module
            .memory_type(memory as u32)
            .expect("memory index is bounded by memory_count");
        validate_memory_type(memory, memory_type.limits.min, memory_type.limits.max)?;
    }
    Ok(())
}

fn validate_memory_type(memory: usize, min: u32, max: Option<u32>) -> Result<(), ValidationError> {
    if min > MAX_MEMORY_PAGES {
        return Err(ValidationError::MemoryPageLimitExceeded { memory, pages: min });
    }
    if let Some(max) = max {
        if max > MAX_MEMORY_PAGES {
            return Err(ValidationError::MemoryPageLimitExceeded { memory, pages: max });
        }
        if min > max {
            return Err(ValidationError::InvalidMemoryLimits { memory, min, max });
        }
    }
    Ok(())
}

fn function_type(module: &Module, function_index: u32) -> Option<&FuncType> {
    let function = function_index as usize;
    let imported = module.function_import_count();
    if function < imported {
        let type_index = module.function_import_type_index(function)? as usize;
        return module.types.get(type_index);
    }
    let defined = function.checked_sub(imported)?;
    let type_index = *module.function_type_indices.get(defined)? as usize;
    module.types.get(type_index)
}

fn ensure_memory(module: &Module, function: usize, offset: usize) -> Result<(), ValidationError> {
    if module.memory_count() == 0 {
        Err(ValidationError::MemoryInstructionWithoutMemory { function, offset })
    } else {
        Ok(())
    }
}

fn natural_alignment(opcode: u8) -> u32 {
    match opcode {
        0x29 | 0x2b | 0x37 | 0x39 => 3,
        0x28 | 0x2a | 0x34 | 0x35 | 0x36 | 0x38 | 0x3e => 2,
        0x2e | 0x2f | 0x32 | 0x33 | 0x3b | 0x3d => 1,
        0x2c | 0x2d | 0x30 | 0x31 | 0x3a | 0x3c => 0,
        _ => unreachable!("natural alignment queried only for supported memory access opcodes"),
    }
}

fn read_memarg(
    code: &[u8],
    pc: &mut usize,
    function: usize,
    offset: usize,
    maximum_alignment: u32,
) -> Result<(u32, u32), ValidationError> {
    let alignment = read_u32_immediate(code, pc, function, offset)?;
    let displacement = read_u32_immediate(code, pc, function, offset)?;
    if alignment > maximum_alignment {
        return Err(ValidationError::InvalidMemoryAlignment {
            function,
            offset,
            alignment,
            maximum: maximum_alignment,
        });
    }
    Ok((alignment, displacement))
}

fn read_memory_index(
    code: &[u8],
    pc: &mut usize,
    module: &Module,
    function: usize,
    offset: usize,
) -> Result<u32, ValidationError> {
    ensure_memory(module, function, offset)?;
    let memory_index = read_u32_immediate(code, pc, function, offset)?;
    if memory_index as usize >= module.memory_count() {
        return Err(ValidationError::MemoryIndexOutOfBounds {
            function,
            offset,
            memory_index,
        });
    }
    Ok(memory_index)
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
    use wasm_parser::{
        DataMode, DataSegment, Export, FuncType, FunctionBody, Import, Limits, MemoryType,
    };

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
            ..Module::default()
        }
    }

    fn with_memory(mut module: Module, min: u32, max: Option<u32>) -> Module {
        module.memories = vec![MemoryType {
            limits: Limits { min, max },
        }];
        module
    }

    fn valid_module() -> Module {
        module_with_code(1, 1, vec![0x20, 0x00, 0x0b])
    }

    fn import(module: &str, name: &str, type_index: u32) -> Import {
        Import {
            module: module.into(),
            name: name.into(),
            desc: ImportDesc::Function(type_index),
        }
    }

    #[test]
    fn accepts_structurally_valid_module() {
        assert_eq!(validate(&valid_module()), Ok(()));
    }

    #[test]
    fn accepts_imported_function_export() {
        let module = Module {
            types: vec![FuncType {
                params: vec![ValueType::I32],
                results: vec![ValueType::I32],
            }],
            imports: vec![import("env", "double", 0)],
            exports: vec![Export {
                name: "double".into(),
                kind: ExportKind::Function,
                index: 0,
            }],
            ..Module::default()
        };
        assert_eq!(validate(&module), Ok(()));
    }

    #[test]
    fn defined_function_can_call_import() {
        let module = Module {
            types: vec![FuncType {
                params: vec![ValueType::I32],
                results: vec![ValueType::I32],
            }],
            imports: vec![import("env", "double", 0)],
            function_type_indices: vec![0],
            exports: vec![Export {
                name: "run".into(),
                kind: ExportKind::Function,
                index: 1,
            }],
            code: vec![FunctionBody {
                locals: vec![],
                code: vec![0x20, 0x00, 0x10, 0x00, 0x0b],
            }],
            ..Module::default()
        };
        assert_eq!(validate(&module), Ok(()));
    }

    #[test]
    fn rejects_bad_import_type_index() {
        let module = Module {
            types: vec![],
            imports: vec![import("env", "f", 2)],
            ..Module::default()
        };
        assert_eq!(
            validate(&module),
            Err(ValidationError::ImportTypeIndexOutOfBounds {
                import: 0,
                type_index: 2,
            })
        );
    }

    #[test]
    fn accepts_all_numeric_import_signature_types() {
        let module = Module {
            types: vec![FuncType {
                params: vec![
                    ValueType::I32,
                    ValueType::I64,
                    ValueType::F32,
                    ValueType::F64,
                ],
                results: vec![ValueType::F64],
            }],
            imports: vec![import("env", "f", 0)],
            ..Module::default()
        };
        assert_eq!(validate(&module), Ok(()));
    }

    #[test]
    fn accepts_memory_load_store_and_grow() {
        let module = with_memory(
            module_with_code(
                2,
                1,
                vec![
                    0x20, 0x00, 0x20, 0x01, 0x36, 0x02, 0x00, 0x20, 0x00, 0x28, 0x02, 0x00, 0x0b,
                ],
            ),
            1,
            Some(2),
        );
        assert_eq!(validate(&module), Ok(()));

        let grow = with_memory(
            module_with_code(1, 1, vec![0x20, 0x00, 0x40, 0x00, 0x0b]),
            1,
            Some(2),
        );
        assert_eq!(validate(&grow), Ok(()));
    }

    #[test]
    fn accepts_data_segment_and_memory_export() {
        let mut module = with_memory(valid_module(), 1, Some(2));
        module.exports.push(Export {
            name: "memory".into(),
            kind: ExportKind::Memory,
            index: 0,
        });
        module.data.push(DataSegment {
            mode: DataMode::Active {
                memory_index: 0,
                offset: 8,
            },
            bytes: b"wasm".to_vec(),
        });
        assert_eq!(validate(&module), Ok(()));
    }

    #[test]
    fn rejects_multiple_memories() {
        let mut module = valid_module();
        module.memories = vec![
            MemoryType {
                limits: Limits { min: 1, max: None },
            },
            MemoryType {
                limits: Limits { min: 1, max: None },
            },
        ];
        assert_eq!(
            validate(&module),
            Err(ValidationError::UnsupportedMemoryCount { count: 2 })
        );
    }

    #[test]
    fn rejects_invalid_memory_limits() {
        let module = with_memory(valid_module(), 3, Some(2));
        assert_eq!(
            validate(&module),
            Err(ValidationError::InvalidMemoryLimits {
                memory: 0,
                min: 3,
                max: 2,
            })
        );
    }

    #[test]
    fn rejects_memory_over_spec_page_limit() {
        let module = with_memory(valid_module(), MAX_MEMORY_PAGES + 1, None);
        assert!(matches!(
            validate(&module),
            Err(ValidationError::MemoryPageLimitExceeded { .. })
        ));
    }

    #[test]
    fn rejects_memory_instruction_without_memory() {
        let module = module_with_code(1, 1, vec![0x20, 0x00, 0x28, 0x02, 0x00, 0x0b]);
        assert!(matches!(
            validate(&module),
            Err(ValidationError::MemoryInstructionWithoutMemory { .. })
        ));
    }

    #[test]
    fn rejects_overaligned_memory_access() {
        let module = with_memory(
            module_with_code(1, 1, vec![0x20, 0x00, 0x28, 0x03, 0x00, 0x0b]),
            1,
            None,
        );
        assert!(matches!(
            validate(&module),
            Err(ValidationError::InvalidMemoryAlignment {
                alignment: 3,
                maximum: 2,
                ..
            })
        ));
    }

    #[test]
    fn rejects_bad_memory_index_immediate() {
        let module = with_memory(module_with_code(0, 1, vec![0x3f, 0x01, 0x0b]), 1, None);
        assert!(matches!(
            validate(&module),
            Err(ValidationError::MemoryIndexOutOfBounds {
                memory_index: 1,
                ..
            })
        ));
    }

    #[test]
    fn rejects_data_segment_without_memory() {
        let mut module = valid_module();
        module.data.push(DataSegment {
            mode: DataMode::Active {
                memory_index: 0,
                offset: 0,
            },
            bytes: vec![1],
        });
        assert!(matches!(
            validate(&module),
            Err(ValidationError::DataMemoryOutOfBounds { .. })
        ));
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
    fn accepts_non_i32_execution_types() {
        let mut module = valid_module();
        module.types[0].params[0] = ValueType::I64;
        module.types[0].results[0] = ValueType::I64;
        assert_eq!(validate(&module), Ok(()));
    }

    #[test]
    fn accepts_typed_if_else_result() {
        let module = module_with_code(
            1,
            1,
            vec![
                0x20, 0x00, 0x04, 0x7f, 0x41, 0x01, 0x05, 0x41, 0x02, 0x0b, 0x0b,
            ],
        );
        assert_eq!(validate(&module), Ok(()));
    }

    #[test]
    fn accepts_block_branch_with_result() {
        let module = module_with_code(0, 1, vec![0x02, 0x7f, 0x41, 0x2a, 0x0c, 0x00, 0x0b, 0x0b]);
        assert_eq!(validate(&module), Ok(()));
    }

    #[test]
    fn accepts_loop_and_conditional_branch() {
        let module = module_with_code(
            1,
            1,
            vec![
                0x03, 0x40, 0x20, 0x00, 0x41, 0x01, 0x6b, 0x22, 0x00, 0x0d, 0x00, 0x0b, 0x20, 0x00,
                0x0b,
            ],
        );
        assert_eq!(validate(&module), Ok(()));
    }

    #[test]
    fn unreachable_code_is_stack_polymorphic_but_still_opcode_checked() {
        let valid = module_with_code(1, 1, vec![0x20, 0x00, 0x0f, 0x6a, 0x0b]);
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
        let module = module_with_code(0, 0, vec![0x02, 0x70, 0x0b, 0x0b]);
        assert!(matches!(
            validate(&module),
            Err(ValidationError::UnsupportedBlockType {
                block_type: 0x70,
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
