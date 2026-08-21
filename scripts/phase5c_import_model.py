from pathlib import Path


def replace_once(text: str, old: str, new: str) -> str:
    assert old in text, old[:120]
    return text.replace(old, new, 1)


def between(text: str, start: str, end: str, replacement: str) -> str:
    a = text.index(start)
    b = text.index(end, a)
    return text[:a] + replacement + text[b:]


# ---------------------------------------------------------------------------
# Parser AST + decoding
# ---------------------------------------------------------------------------
path = Path("crates/wasm-parser/src/lib.rs")
text = path.read_text()

text = replace_once(
    text,
    '''#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    pub module: String,
    pub name: String,
    pub type_index: u32,
}

''',
    '',
)

insert_after = '''#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlobalType {
    pub value_type: ValueType,
    pub mutable: bool,
}
'''
import_types = insert_after + '''
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    Function,
    Table,
    Memory,
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportDesc {
    Function(u32),
    Table(TableType),
    Memory(MemoryType),
    Global(GlobalType),
}

impl ImportDesc {
    pub fn kind(self) -> ImportKind {
        match self {
            Self::Function(_) => ImportKind::Function,
            Self::Table(_) => ImportKind::Table,
            Self::Memory(_) => ImportKind::Memory,
            Self::Global(_) => ImportKind::Global,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    pub module: String,
    pub name: String,
    pub desc: ImportDesc,
}

impl Import {
    pub fn kind(&self) -> ImportKind {
        self.desc.kind()
    }

    pub fn function_type_index(&self) -> Option<u32> {
        match self.desc {
            ImportDesc::Function(type_index) => Some(type_index),
            _ => None,
        }
    }
}
'''
text = replace_once(text, insert_after, import_types)

text = replace_once(
    text,
    '''    /// Function imports occupy the first entries in the module function index space.
    pub imports: Vec<Import>,
''',
    '''    /// Imports retain binary-section order; each descriptor belongs to its own index space.
    pub imports: Vec<Import>,
''',
)

module_end = '''    pub data: Vec<DataSegment>,
}

/// Decode a canonical-or-noncanonical unsigned LEB128 u32 value.
'''
module_helpers = '''    pub data: Vec<DataSegment>,
}

impl Module {
    pub fn function_imports(&self) -> impl Iterator<Item = &Import> {
        self.imports
            .iter()
            .filter(|import| matches!(import.desc, ImportDesc::Function(_)))
    }

    pub fn function_import_count(&self) -> usize {
        self.function_imports().count()
    }

    pub fn table_import_count(&self) -> usize {
        self.imports
            .iter()
            .filter(|import| matches!(import.desc, ImportDesc::Table(_)))
            .count()
    }

    pub fn memory_import_count(&self) -> usize {
        self.imports
            .iter()
            .filter(|import| matches!(import.desc, ImportDesc::Memory(_)))
            .count()
    }

    pub fn global_import_count(&self) -> usize {
        self.imports
            .iter()
            .filter(|import| matches!(import.desc, ImportDesc::Global(_)))
            .count()
    }

    pub fn function_import(&self, index: usize) -> Option<&Import> {
        self.function_imports().nth(index)
    }

    pub fn function_import_type_index(&self, index: usize) -> Option<u32> {
        self.function_import(index)?.function_type_index()
    }

    pub fn function_count(&self) -> usize {
        self.function_import_count() + self.function_type_indices.len()
    }

    pub fn table_count(&self) -> usize {
        self.table_import_count() + self.tables.len()
    }

    pub fn memory_count(&self) -> usize {
        self.memory_import_count() + self.memories.len()
    }

    pub fn global_count(&self) -> usize {
        self.global_import_count() + self.globals.len()
    }

    pub fn table_type(&self, index: u32) -> Option<TableType> {
        let mut imported = self.imports.iter().filter_map(|import| match import.desc {
            ImportDesc::Table(ty) => Some(ty),
            _ => None,
        });
        let imported_count = self.table_import_count();
        let index = index as usize;
        if index < imported_count {
            return imported.nth(index);
        }
        self.tables.get(index.checked_sub(imported_count)?).copied()
    }

    pub fn memory_type(&self, index: u32) -> Option<MemoryType> {
        let mut imported = self.imports.iter().filter_map(|import| match import.desc {
            ImportDesc::Memory(ty) => Some(ty),
            _ => None,
        });
        let imported_count = self.memory_import_count();
        let index = index as usize;
        if index < imported_count {
            return imported.nth(index);
        }
        self.memories.get(index.checked_sub(imported_count)?).copied()
    }

    pub fn global_type(&self, index: u32) -> Option<GlobalType> {
        let mut imported = self.imports.iter().filter_map(|import| match import.desc {
            ImportDesc::Global(ty) => Some(ty),
            _ => None,
        });
        let imported_count = self.global_import_count();
        let index = index as usize;
        if index < imported_count {
            return imported.nth(index);
        }
        self.globals
            .get(index.checked_sub(imported_count)?)
            .map(|global| global.ty)
    }
}

/// Decode a canonical-or-noncanonical unsigned LEB128 u32 value.
'''
text = replace_once(text, module_end, module_helpers)

old_import_parser = '''fn parse_import_section(cursor: &mut Cursor<'_>, module: &mut Module) -> Result<(), ParseError> {
    let count = cursor.read_u32()?;
    module.imports.reserve(count as usize);
    for _ in 0..count {
        let import_module = cursor.read_name()?;
        let name = cursor.read_name()?;
        let kind = cursor.read_u8()?;
        if kind != 0x00 {
            return Err(ParseError::InvalidImportKind(kind));
        }
        let type_index = cursor.read_u32()?;
        module.imports.push(Import {
            module: import_module,
            name,
            type_index,
        });
    }
    Ok(())
}
'''
new_import_parser = '''fn parse_import_section(cursor: &mut Cursor<'_>, module: &mut Module) -> Result<(), ParseError> {
    let count = cursor.read_u32()?;
    module.imports.reserve(count as usize);
    for _ in 0..count {
        let import_module = cursor.read_name()?;
        let name = cursor.read_name()?;
        let desc = match cursor.read_u8()? {
            0x00 => ImportDesc::Function(cursor.read_u32()?),
            0x01 => {
                let reference_type = cursor.read_u8()?;
                if reference_type != 0x70 {
                    return Err(ParseError::InvalidReferenceType(reference_type));
                }
                ImportDesc::Table(TableType {
                    limits: read_limits(cursor)?,
                })
            }
            0x02 => ImportDesc::Memory(MemoryType {
                limits: read_limits(cursor)?,
            }),
            0x03 => {
                let value_type = read_value_type(cursor)?;
                let mutable = read_mutability(cursor)?;
                ImportDesc::Global(GlobalType {
                    value_type,
                    mutable,
                })
            }
            other => return Err(ParseError::InvalidImportKind(other)),
        };
        module.imports.push(Import {
            module: import_module,
            name,
            desc,
        });
    }
    Ok(())
}
'''
text = replace_once(text, old_import_parser, new_import_parser)

old_global_mut = '''        let value_type = read_value_type(cursor)?;
        let mutable = match cursor.read_u8()? {
            0 => false,
            1 => true,
            other => return Err(ParseError::InvalidMutability(other)),
        };
'''
new_global_mut = '''        let value_type = read_value_type(cursor)?;
        let mutable = read_mutability(cursor)?;
'''
text = replace_once(text, old_global_mut, new_global_mut)

limits_marker = '''fn read_limits(cursor: &mut Cursor<'_>) -> Result<Limits, ParseError> {
'''
mut_helper = '''fn read_mutability(cursor: &mut Cursor<'_>) -> Result<bool, ParseError> {
    match cursor.read_u8()? {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(ParseError::InvalidMutability(other)),
    }
}

'''
assert limits_marker in text
text = text.replace(limits_marker, mut_helper + limits_marker, 1)

text = text.replace(
    '"unsupported import kind {kind}; this milestone supports function imports only"',
    '"invalid import kind {kind}"',
    1,
)

# Parser tests: function descriptor + object import descriptors + invalid kind.
text = text.replace('assert_eq!(module.imports[0].type_index, 0);', 'assert_eq!(module.imports[0].desc, ImportDesc::Function(0));', 1)
old_test = '''    #[test]
    fn rejects_non_function_import() {
        let mut bytes = imported_function_module();
        let kind_offset = bytes.len() - 2;
        bytes[kind_offset] = 0x02;
        assert_eq!(
            parse_module(&bytes),
            Err(ParseError::InvalidImportKind(0x02))
        );
    }
'''
new_test = '''    #[test]
    fn parses_non_function_import_descriptors_and_independent_counts() {
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        push_section(&mut bytes, 1, &[0x01, 0x60, 0x00, 0x00]);
        let imports = [
            0x04,
            0x03, b'e', b'n', b'v', 0x03, b'm', b'e', b'm', 0x02, 0x00, 0x01,
            0x03, b'e', b'n', b'v', 0x03, b't', b'a', b'b', 0x01, 0x70, 0x00, 0x02,
            0x03, b'e', b'n', b'v', 0x01, b'g', 0x03, 0x7e, 0x00,
            0x03, b'e', b'n', b'v', 0x01, b'f', 0x00, 0x00,
        ];
        push_section(&mut bytes, 2, &imports);
        let module = parse_module(&bytes).expect("all MVP import descriptors parse");
        assert_eq!(module.function_import_count(), 1);
        assert_eq!(module.table_import_count(), 1);
        assert_eq!(module.memory_import_count(), 1);
        assert_eq!(module.global_import_count(), 1);
        assert_eq!(module.function_import(0).unwrap().name, "f");
        assert!(matches!(module.imports[0].desc, ImportDesc::Memory(_)));
        assert!(matches!(module.imports[1].desc, ImportDesc::Table(_)));
        assert!(matches!(module.imports[2].desc, ImportDesc::Global(_)));
        assert!(matches!(module.imports[3].desc, ImportDesc::Function(0)));
    }

    #[test]
    fn rejects_invalid_import_kind() {
        let mut bytes = imported_function_module();
        let kind_offset = bytes.len() - 2;
        bytes[kind_offset] = 0x04;
        assert_eq!(parse_module(&bytes), Err(ParseError::InvalidImportKind(0x04)));
    }
'''
text = replace_once(text, old_test, new_test)
path.write_text(text)


# ---------------------------------------------------------------------------
# Validator: independent function/table/memory/global index spaces.
# ---------------------------------------------------------------------------
path = Path("crates/wasm-validator/src/lib.rs")
text = path.read_text()
text = text.replace(
    'use wasm_parser::{decode_u32, ExportKind, FuncType, Module, ValueType};',
    'use wasm_parser::{decode_u32, ExportKind, FuncType, ImportDesc, Module, ValueType};',
    1,
)
text = text.replace('module.imports.len() + defined', 'module.function_import_count() + defined')
text = text.replace(
    'let total_functions = module.imports.len() + module.function_type_indices.len();',
    'let total_functions = module.function_count();',
    1,
)
text = text.replace('export.index as usize >= module.memories.len()', 'export.index as usize >= module.memory_count()', 1)
text = text.replace('export.index as usize >= module.tables.len()', 'export.index as usize >= module.table_count()', 1)
text = text.replace('data.memory_index as usize >= module.memories.len()', 'data.memory_index as usize >= module.memory_count()', 1)

old_validate_imports = '''fn validate_imports(module: &Module) -> Result<(), ValidationError> {
    for (import, entry) in module.imports.iter().enumerate() {
        let Some(function_type) = module.types.get(entry.type_index as usize) else {
            return Err(ValidationError::ImportTypeIndexOutOfBounds {
                import,
                type_index: entry.type_index,
            });
        };
        if function_type.results.len() > 1 {
            return Err(ValidationError::UnsupportedImportResultArity {
                import,
                results: function_type.results.len(),
            });
        }
        for &value_type in function_type
            .params
            .iter()
            .chain(function_type.results.iter())
        {
            if value_type != ValueType::I32 {
                return Err(ValidationError::UnsupportedImportValueType { import, value_type });
            }
        }
    }
    Ok(())
}
'''
new_validate_imports = '''fn validate_imports(module: &Module) -> Result<(), ValidationError> {
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
                for &value_type in function_type
                    .params
                    .iter()
                    .chain(function_type.results.iter())
                {
                    if value_type != ValueType::I32 {
                        return Err(ValidationError::UnsupportedImportValueType { import, value_type });
                    }
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
'''
text = replace_once(text, old_validate_imports, new_validate_imports)

old_memories = '''fn validate_memories(module: &Module) -> Result<(), ValidationError> {
    if module.memories.len() > 1 {
        return Err(ValidationError::UnsupportedMemoryCount {
            count: module.memories.len(),
        });
    }

    for (memory, memory_type) in module.memories.iter().enumerate() {
        let limits = memory_type.limits;
        if limits.min > MAX_MEMORY_PAGES {
            return Err(ValidationError::MemoryPageLimitExceeded {
                memory,
                pages: limits.min,
            });
        }
        if let Some(max) = limits.max {
            if max > MAX_MEMORY_PAGES {
                return Err(ValidationError::MemoryPageLimitExceeded { memory, pages: max });
            }
            if limits.min > max {
                return Err(ValidationError::InvalidMemoryLimits {
                    memory,
                    min: limits.min,
                    max,
                });
            }
        }
    }
    Ok(())
}
'''
new_memories = '''fn validate_memories(module: &Module) -> Result<(), ValidationError> {
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

fn validate_memory_type(
    memory: usize,
    min: u32,
    max: Option<u32>,
) -> Result<(), ValidationError> {
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
'''
text = replace_once(text, old_memories, new_memories)

old_function_type = '''fn function_type(module: &Module, function_index: u32) -> Option<&FuncType> {
    let function = function_index as usize;
    if function < module.imports.len() {
        let type_index = module.imports[function].type_index as usize;
        return module.types.get(type_index);
    }
    let defined = function.checked_sub(module.imports.len())?;
    let type_index = *module.function_type_indices.get(defined)? as usize;
    module.types.get(type_index)
}
'''
new_function_type = '''fn function_type(module: &Module, function_index: u32) -> Option<&FuncType> {
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
'''
text = replace_once(text, old_function_type, new_function_type)
text = text.replace('if module.memories.is_empty() {', 'if module.memory_count() == 0 {', 1)
text = text.replace('memory_index as usize >= module.memories.len()', 'memory_index as usize >= module.memory_count()', 1)

# validator test helper uses descriptor enum
text = text.replace(
    '''        Import {
            module: module.into(),
            name: name.into(),
            type_index,
        }
''',
    '''        Import {
            module: module.into(),
            name: name.into(),
            desc: ImportDesc::Function(type_index),
        }
''',
    1,
)
path.write_text(text)


# Phase 5 table/element/global export index spaces.
path = Path("crates/wasm-validator/src/phase5.rs")
text = path.read_text()
text = text.replace('if module.tables.len() > 1 {', 'if module.table_count() > 1 {', 1)
text = text.replace('count: module.tables.len(),', 'count: module.table_count(),', 1)
old_table_loop = '''    for (table, table_type) in module.tables.iter().enumerate() {
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
'''
new_table_loop = '''    for table in 0..module.table_count() {
        let table_type = module
            .table_type(table as u32)
            .expect("table index is bounded by table_count");
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
'''
text = replace_once(text, old_table_loop, new_table_loop)
text = text.replace(
    'let total_functions = module.imports.len() + module.function_type_indices.len();',
    'let total_functions = module.function_count();',
    1,
)
text = text.replace('element.table_index as usize >= module.tables.len()', 'element.table_index as usize >= module.table_count()', 1)
text = text.replace('(index as usize) < module.globals.len()', '(index as usize) < module.global_count()', 1)
path.write_text(text)


# Typed validator state/table access resolves imported objects through index-space helpers.
path = Path("crates/wasm-validator/src/typed.rs")
text = path.read_text()
old_global_get = '''                let Some(global) = module.globals.get(global_index as usize) else {
                    return Err(ValidationError::GlobalIndexOutOfBounds {
                        function,
                        offset,
                        global_index,
                    });
                };
                stack.push(global.ty.value_type);
'''
new_global_get = '''                let Some(global_type) = module.global_type(global_index) else {
                    return Err(ValidationError::GlobalIndexOutOfBounds {
                        function,
                        offset,
                        global_index,
                    });
                };
                stack.push(global_type.value_type);
'''
text = replace_once(text, old_global_get, new_global_get)
old_global_set = '''                let Some(global) = module.globals.get(global_index as usize) else {
                    return Err(ValidationError::GlobalIndexOutOfBounds {
                        function,
                        offset,
                        global_index,
                    });
                };
                if !global.ty.mutable {
                    return Err(ValidationError::ImmutableGlobalSet {
                        function,
                        offset,
                        global_index,
                    });
                }
                pop_expect(
                    &mut stack,
                    &controls,
                    global.ty.value_type,
                    function,
                    offset,
                )?;
'''
new_global_set = '''                let Some(global_type) = module.global_type(global_index) else {
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
'''
text = replace_once(text, old_global_set, new_global_set)
text = text.replace('table_index as usize >= module.tables.len()', 'table_index as usize >= module.table_count()', 1)
path.write_text(text)


# ---------------------------------------------------------------------------
# Runtime: function-import ordinal mapping; explicit object-import rejection.
# ---------------------------------------------------------------------------
path = Path("crates/wasm-runtime/src/lib.rs")
text = path.read_text()
text = text.replace(
    'ParseError, ValueType,',
    'ImportDesc, ImportKind, ParseError, ValueType,',
    1,
)
error_marker = '''    HostSignatureMismatch {
        module: String,
        name: String,
    },
'''
error_insert = '''    UnsupportedObjectImport {
        module: String,
        name: String,
        kind: ImportKind,
    },
''' + error_marker
text = replace_once(text, error_marker, error_insert)
display_marker = '''            Self::HostSignatureMismatch { module, name } => write!(
'''
display_insert = '''            Self::UnsupportedObjectImport { module, name, kind } => write!(
                f,
                "import {module}.{name} has unsupported runtime object kind {kind:?}; shared object imports are not instantiated yet"
            ),
''' + display_marker
text = replace_once(text, display_marker, display_insert)
text = replace_once(
    text,
    '''        validate(&module)?;
        validate_host_bindings(&module, &hosts)?;
''',
    '''        validate(&module)?;
        reject_unsupported_object_imports(&module)?;
        validate_host_bindings(&module, &hosts)?;
''',
)
old_rt_function_type = '''        let function = function_index as usize;
        let type_index = if function < self.module.imports.len() {
            self.module.imports[function].type_index
        } else {
            let defined = function
                .checked_sub(self.module.imports.len())
'''
new_rt_function_type = '''        let function = function_index as usize;
        let imported = self.module.function_import_count();
        let type_index = if function < imported {
            self.module
                .function_import_type_index(function)
                .ok_or(RuntimeError::FunctionOutOfBounds(function_index))?
        } else {
            let defined = function
                .checked_sub(imported)
'''
text = replace_once(text, old_rt_function_type, new_rt_function_type)

old_invoke_host = '''        let import = self.module.imports[import_index].clone();
        let ty = self.module.types[import.type_index as usize].clone();
'''
new_invoke_host = '''        let import = self
            .module
            .function_import(import_index)
            .ok_or(RuntimeError::FunctionOutOfBounds(import_index as u32))?
            .clone();
        let type_index = import
            .function_type_index()
            .ok_or(RuntimeError::FunctionOutOfBounds(import_index as u32))?;
        let ty = self.module.types[type_index as usize].clone();
'''
text = replace_once(text, old_invoke_host, new_invoke_host)
old_invoke_function = '''        let function = function_index as usize;
        if function < self.module.imports.len() {
            return self.invoke_host(function, args, budget);
        }
'''
new_invoke_function = '''        let function = function_index as usize;
        let imported = self.module.function_import_count();
        if function < imported {
            return self.invoke_host(function, args, budget);
        }
'''
text = replace_once(text, old_invoke_function, new_invoke_function)
text = text.replace('.checked_sub(self.module.imports.len())', '.checked_sub(imported)', 1)

old_host_bindings = '''fn validate_host_bindings(module: &Module, hosts: &HostRegistry) -> Result<(), RuntimeError> {
    for import in &module.imports {
        let key = (import.module.clone(), import.name.clone());
        let host = hosts
            .functions
            .get(&key)
            .ok_or_else(|| RuntimeError::UnresolvedImport {
                module: import.module.clone(),
                name: import.name.clone(),
            })?;
        let declared = &module.types[import.type_index as usize];
        if host.params != declared.params || host.results != declared.results {
            return Err(RuntimeError::HostSignatureMismatch {
                module: import.module.clone(),
                name: import.name.clone(),
            });
        }
    }
    Ok(())
}
'''
new_host_bindings = '''fn reject_unsupported_object_imports(module: &Module) -> Result<(), RuntimeError> {
    for import in &module.imports {
        if !matches!(import.desc, ImportDesc::Function(_)) {
            return Err(RuntimeError::UnsupportedObjectImport {
                module: import.module.clone(),
                name: import.name.clone(),
                kind: import.kind(),
            });
        }
    }
    Ok(())
}

fn validate_host_bindings(module: &Module, hosts: &HostRegistry) -> Result<(), RuntimeError> {
    for import in module.function_imports() {
        let key = (import.module.clone(), import.name.clone());
        let host = hosts
            .functions
            .get(&key)
            .ok_or_else(|| RuntimeError::UnresolvedImport {
                module: import.module.clone(),
                name: import.name.clone(),
            })?;
        let type_index = import
            .function_type_index()
            .expect("function_imports yields only function descriptors");
        let declared = &module.types[type_index as usize];
        if host.params != declared.params || host.results != declared.results {
            return Err(RuntimeError::HostSignatureMismatch {
                module: import.module.clone(),
                name: import.name.clone(),
            });
        }
    }
    Ok(())
}
'''
text = replace_once(text, old_host_bindings, new_host_bindings)

# runtime unit test constructor
text = text.replace(
    '''            imports: vec![Import {
                module: "env".into(),
                name: "poke".into(),
                type_index: 0,
            }],
''',
    '''            imports: vec![Import {
                module: "env".into(),
                name: "poke".into(),
                desc: ImportDesc::Function(0),
            }],
''',
    1,
)
path.write_text(text)


# CLI inspect understands every import descriptor and shows independent counts.
path = Path("crates/wasm-cli/src/main.rs")
text = path.read_text()
text = text.replace('use wasm_parser::parse_module;', 'use wasm_parser::{parse_module, ImportDesc};', 1)
old_cli_imports = '''    println!("imports: {}", module.imports.len());
    for (index, import) in module.imports.iter().enumerate() {
        println!(
            "  function #{index}: {}.{} type #{}",
            import.module, import.name, import.type_index
        );
    }
    println!("defined functions: {}", module.function_type_indices.len());
    println!(
        "function index space: {}",
        module.imports.len() + module.function_type_indices.len()
    );
'''
new_cli_imports = '''    println!("imports: {}", module.imports.len());
    for import in &module.imports {
        match import.desc {
            ImportDesc::Function(type_index) => {
                println!("  function: {}.{} type #{type_index}", import.module, import.name);
            }
            ImportDesc::Table(table) => println!(
                "  table: {}.{} funcref min={} max={:?}",
                import.module, import.name, table.limits.min, table.limits.max
            ),
            ImportDesc::Memory(memory) => println!(
                "  memory: {}.{} min={} max={:?}",
                import.module, import.name, memory.limits.min, memory.limits.max
            ),
            ImportDesc::Global(global) => println!(
                "  global: {}.{} {:?} mutable={}",
                import.module, import.name, global.value_type, global.mutable
            ),
        }
    }
    println!("defined functions: {}", module.function_type_indices.len());
    println!("function index space: {}", module.function_count());
    println!("table index space: {}", module.table_count());
    println!("memory index space: {}", module.memory_count());
    println!("global index space: {}", module.global_count());
'''
text = replace_once(text, old_cli_imports, new_cli_imports)
path.write_text(text)
