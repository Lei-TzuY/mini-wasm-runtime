from pathlib import Path


def replace_once(path, old, new, label):
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one anchor, found {count}")
    p.write_text(text.replace(old, new, 1))


parser = "crates/wasm-parser/src/lib.rs"
validator = "crates/wasm-validator/src/lib.rs"
phase5 = "crates/wasm-validator/src/phase5.rs"
runtime = "crates/wasm-runtime/src/lib.rs"

replace_once(
    parser,
    "    InvalidReferenceType(u8),\n    InvalidMutability(u8),\n    UnsupportedElementSegmentMode(u32),\n",
    "    InvalidReferenceType(u8),\n    InvalidMutability(u8),\n    InvalidElementKind(u8),\n    UnsupportedElementSegmentMode(u32),\n",
    "parser error enum",
)

replace_once(
    parser,
    "            Self::InvalidMutability(value) => write!(f, \"invalid global mutability byte {value}\"),\n            Self::UnsupportedElementSegmentMode(mode) => {\n",
    "            Self::InvalidMutability(value) => write!(f, \"invalid global mutability byte {value}\"),\n            Self::InvalidElementKind(kind) => {\n                write!(f, \"invalid legacy element kind byte 0x{kind:02x}\")\n            }\n            Self::UnsupportedElementSegmentMode(mode) => {\n",
    "parser error display",
)

replace_once(
    parser,
    '''#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementSegment {
    pub table_index: u32,
    pub offset: i32,
    pub function_indices: Vec<u32>,
}
''',
    '''#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementMode {
    Active { table_index: u32, offset: i32 },
    Passive,
    Declarative,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementSegment {
    pub mode: ElementMode,
    pub function_indices: Vec<u32>,
}
''',
    "element AST",
)

replace_once(
    parser,
    '''#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataSegment {
    /// Active segments currently target memory 0 only.
    pub memory_index: u32,
    /// Constant i32 byte offset evaluated during instantiation.
    pub offset: i32,
    pub bytes: Vec<u8>,
}
''',
    '''#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataMode {
    Active { memory_index: u32, offset: i32 },
    Passive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataSegment {
    pub mode: DataMode,
    pub bytes: Vec<u8>,
}
''',
    "data AST",
)

old_element_parser = '''fn parse_element_section(cursor: &mut Cursor<'_>, module: &mut Module) -> Result<(), ParseError> {
    let count = cursor.read_u32()?;
    module.elements.reserve(count as usize);
    for _ in 0..count {
        let mode = cursor.read_u32()?;
        if mode != 0 {
            return Err(ParseError::UnsupportedElementSegmentMode(mode));
        }
        let offset = read_i32_const_expr(cursor)?;
        let function_count = cursor.read_u32()?;
        let mut function_indices = Vec::with_capacity(function_count as usize);
        for _ in 0..function_count {
            function_indices.push(cursor.read_u32()?);
        }
        module.elements.push(ElementSegment {
            table_index: 0,
            offset,
            function_indices,
        });
    }
    Ok(())
}
'''
new_element_parser = '''fn parse_element_section(cursor: &mut Cursor<'_>, module: &mut Module) -> Result<(), ParseError> {
    let count = cursor.read_u32()?;
    module.elements.reserve(count as usize);
    for _ in 0..count {
        let flags = cursor.read_u32()?;
        let mode = match flags {
            0 => ElementMode::Active {
                table_index: 0,
                offset: read_i32_const_expr(cursor)?,
            },
            1 => {
                read_legacy_element_kind(cursor)?;
                ElementMode::Passive
            }
            2 => {
                let table_index = cursor.read_u32()?;
                let offset = read_i32_const_expr(cursor)?;
                read_legacy_element_kind(cursor)?;
                ElementMode::Active {
                    table_index,
                    offset,
                }
            }
            3 => {
                read_legacy_element_kind(cursor)?;
                ElementMode::Declarative
            }
            other => return Err(ParseError::UnsupportedElementSegmentMode(other)),
        };
        let function_count = cursor.read_u32()?;
        let mut function_indices = Vec::with_capacity(function_count as usize);
        for _ in 0..function_count {
            function_indices.push(cursor.read_u32()?);
        }
        module.elements.push(ElementSegment {
            mode,
            function_indices,
        });
    }
    Ok(())
}

fn read_legacy_element_kind(cursor: &mut Cursor<'_>) -> Result<(), ParseError> {
    let kind = cursor.read_u8()?;
    if kind == 0x00 {
        Ok(())
    } else {
        Err(ParseError::InvalidElementKind(kind))
    }
}
'''
replace_once(parser, old_element_parser, new_element_parser, "element parser")

old_data_parser = '''fn parse_data_section(cursor: &mut Cursor<'_>, module: &mut Module) -> Result<(), ParseError> {
    let count = cursor.read_u32()?;
    module.data.reserve(count as usize);
    for _ in 0..count {
        let mode = cursor.read_u32()?;
        if mode != 0 {
            return Err(ParseError::UnsupportedDataSegmentMode(mode));
        }
        let offset = read_i32_const_expr(cursor)?;
        let len = cursor.read_u32()? as usize;
        let bytes = cursor.read_exact(len)?.to_vec();
        module.data.push(DataSegment {
            memory_index: 0,
            offset,
            bytes,
        });
    }
    Ok(())
}
'''
new_data_parser = '''fn parse_data_section(cursor: &mut Cursor<'_>, module: &mut Module) -> Result<(), ParseError> {
    let count = cursor.read_u32()?;
    module.data.reserve(count as usize);
    for _ in 0..count {
        let flags = cursor.read_u32()?;
        let mode = match flags {
            0 => DataMode::Active {
                memory_index: 0,
                offset: read_i32_const_expr(cursor)?,
            },
            1 => DataMode::Passive,
            2 => DataMode::Active {
                memory_index: cursor.read_u32()?,
                offset: read_i32_const_expr(cursor)?,
            },
            other => return Err(ParseError::UnsupportedDataSegmentMode(other)),
        };
        let len = cursor.read_u32()? as usize;
        let bytes = cursor.read_exact(len)?.to_vec();
        module.data.push(DataSegment { mode, bytes });
    }
    Ok(())
}
'''
replace_once(parser, old_data_parser, new_data_parser, "data parser")

replace_once(
    parser,
    '''        assert_eq!(module.start, Some(0));
        assert_eq!(module.elements[0].offset, 1);
        assert_eq!(module.elements[0].function_indices, vec![0]);
''',
    '''        assert_eq!(module.start, Some(0));
        assert_eq!(
            module.elements[0].mode,
            ElementMode::Active {
                table_index: 0,
                offset: 1,
            }
        );
        assert_eq!(module.elements[0].function_indices, vec![0]);
''',
    "element parser test assertion",
)

replace_once(
    parser,
    '''    fn rejects_unsupported_element_mode() {
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        push_section(&mut bytes, 9, &[0x01, 0x01]);
        assert_eq!(
            parse_module(&bytes),
            Err(ParseError::UnsupportedElementSegmentMode(1))
        );
    }
''',
    '''    fn rejects_expression_based_element_mode() {
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        push_section(&mut bytes, 9, &[0x01, 0x04]);
        assert_eq!(
            parse_module(&bytes),
            Err(ParseError::UnsupportedElementSegmentMode(4))
        );
    }

    #[test]
    fn rejects_invalid_legacy_element_kind() {
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        push_section(&mut bytes, 9, &[0x01, 0x01, 0x01]);
        assert_eq!(parse_module(&bytes), Err(ParseError::InvalidElementKind(1)));
    }

    #[test]
    fn parses_passive_explicit_and_declarative_element_modes() {
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        let elements = [
            0x03,
            0x01, 0x00, 0x01, 0x00,
            0x02, 0x00, 0x41, 0x02, 0x0b, 0x00, 0x01, 0x00,
            0x03, 0x00, 0x01, 0x00,
        ];
        push_section(&mut bytes, 9, &elements);
        let module = parse_module(&bytes).expect("legacy element modes parse");
        assert_eq!(module.elements[0].mode, ElementMode::Passive);
        assert_eq!(
            module.elements[1].mode,
            ElementMode::Active {
                table_index: 0,
                offset: 2,
            }
        );
        assert_eq!(module.elements[2].mode, ElementMode::Declarative);
        assert!(module
            .elements
            .iter()
            .all(|segment| segment.function_indices == vec![0]));
    }
''',
    "element mode tests",
)

replace_once(
    parser,
    '''        assert_eq!(module.memories[0].limits.min, 1);
        assert_eq!(module.memories[0].limits.max, Some(2));
        assert_eq!(module.data[0].offset, 4);
        assert_eq!(module.data[0].bytes, b"was");
''',
    '''        assert_eq!(module.memories[0].limits.min, 1);
        assert_eq!(module.memories[0].limits.max, Some(2));
        assert_eq!(
            module.data[0].mode,
            DataMode::Active {
                memory_index: 0,
                offset: 4,
            }
        );
        assert_eq!(module.data[0].bytes, b"was");
''',
    "data parser test assertion",
)

replace_once(
    parser,
    '''    fn rejects_unsupported_data_segment_mode() {
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        bytes.extend([0x0b, 0x03, 0x01, 0x01, 0x00]);
        assert_eq!(
            parse_module(&bytes),
            Err(ParseError::UnsupportedDataSegmentMode(1))
        );
    }
''',
    '''    fn rejects_unsupported_data_segment_mode() {
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        bytes.extend([0x0b, 0x03, 0x01, 0x03, 0x00]);
        assert_eq!(
            parse_module(&bytes),
            Err(ParseError::UnsupportedDataSegmentMode(3))
        );
    }

    #[test]
    fn parses_passive_and_explicit_data_modes() {
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        let data = [
            0x02,
            0x01, 0x03, b'p', b'a', b's',
            0x02, 0x00, 0x41, 0x05, 0x0b, 0x03, b'a', b'c', b't',
        ];
        push_section(&mut bytes, 11, &data);
        let module = parse_module(&bytes).expect("data modes parse");
        assert_eq!(module.data[0].mode, DataMode::Passive);
        assert_eq!(module.data[0].bytes, b"pas");
        assert_eq!(
            module.data[1].mode,
            DataMode::Active {
                memory_index: 0,
                offset: 5,
            }
        );
        assert_eq!(module.data[1].bytes, b"act");
    }
''',
    "data mode tests",
)

replace_once(
    validator,
    "use wasm_parser::{decode_u32, ExportKind, FuncType, ImportDesc, Module, ValueType};\n",
    "use wasm_parser::{decode_u32, DataMode, ExportKind, FuncType, ImportDesc, Module, ValueType};\n",
    "validator DataMode import",
)

replace_once(
    validator,
    '''    for (segment, data) in module.data.iter().enumerate() {
        if data.memory_index as usize >= module.memory_count() {
            return Err(ValidationError::DataMemoryOutOfBounds {
                segment,
                memory_index: data.memory_index,
            });
        }
    }
''',
    '''    for (segment, data) in module.data.iter().enumerate() {
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
''',
    "validator data modes",
)

replace_once(
    validator,
    "    use wasm_parser::{DataSegment, Export, FuncType, FunctionBody, Import, Limits, MemoryType};\n",
    "    use wasm_parser::{DataMode, DataSegment, Export, FuncType, FunctionBody, Import, Limits, MemoryType};\n",
    "validator test import",
)

replace_once(
    validator,
    '''        module.data.push(DataSegment {
            memory_index: 0,
            offset: 8,
            bytes: b"wasm".to_vec(),
        });
''',
    '''        module.data.push(DataSegment {
            mode: DataMode::Active {
                memory_index: 0,
                offset: 8,
            },
            bytes: b"wasm".to_vec(),
        });
''',
    "validator active data fixture",
)

replace_once(
    validator,
    '''        module.data.push(DataSegment {
            memory_index: 0,
            offset: 0,
            bytes: vec![1],
        });
''',
    '''        module.data.push(DataSegment {
            mode: DataMode::Active {
                memory_index: 0,
                offset: 0,
            },
            bytes: vec![1],
        });
''',
    "validator missing memory fixture",
)

replace_once(
    phase5,
    "use wasm_parser::Module;\n",
    "use wasm_parser::{ElementMode, Module};\n",
    "phase5 ElementMode import",
)

replace_once(
    phase5,
    '''    for (segment, element) in module.elements.iter().enumerate() {
        if element.table_index as usize >= module.table_count() {
            return Err(ValidationError::ElementTableOutOfBounds {
                segment,
                table_index: element.table_index,
            });
        }
        for &function_index in &element.function_indices {
''',
    '''    for (segment, element) in module.elements.iter().enumerate() {
        if let ElementMode::Active { table_index, .. } = element.mode {
            if table_index as usize >= module.table_count() {
                return Err(ValidationError::ElementTableOutOfBounds {
                    segment,
                    table_index,
                });
            }
        }
        for &function_index in &element.function_indices {
''',
    "phase5 element modes",
)

replace_once(
    runtime,
    '''use wasm_parser::{
    decode_i32, decode_i64, decode_s33, decode_u32, Constant, ExportKind, FuncType, ImportDesc,
    ImportKind, Module, ParseError, ValueType,
};
''',
    '''use wasm_parser::{
    decode_i32, decode_i64, decode_s33, decode_u32, Constant, DataMode, ElementMode, ExportKind,
    FuncType, ImportDesc, ImportKind, Module, ParseError, ValueType,
};
''',
    "runtime segment mode imports",
)

old_runtime_segments = '''    fn initialize_data_segments(&mut self) -> Result<(), RuntimeError> {
        let data = self.module.data.clone();

        // Preflight all active segments before mutating a potentially host-shared memory.
        for (segment_index, segment) in data.iter().enumerate() {
            let offset = u64::from(segment.offset as u32);
            let end = offset.checked_add(segment.bytes.len() as u64).ok_or(
                RuntimeError::DataSegmentOutOfBounds {
                    segment: segment_index,
                    offset,
                    length: segment.bytes.len(),
                },
            )?;
            let memory_len = self.with_memory(|memory| Ok(memory.bytes.len() as u64))?;
            if end > memory_len {
                return Err(RuntimeError::DataSegmentOutOfBounds {
                    segment: segment_index,
                    offset,
                    length: segment.bytes.len(),
                });
            }
        }

        for segment in &data {
            let offset = u64::from(segment.offset as u32);
            self.with_memory_mut(|memory| {
                let start = usize::try_from(offset).map_err(|_| {
                    RuntimeError::ControlInvariant("preflighted data offset no longer fits usize")
                })?;
                let end = start + segment.bytes.len();
                memory.bytes[start..end].copy_from_slice(&segment.bytes);
                Ok(())
            })?;
        }
        Ok(())
    }

    fn initialize_element_segments(&mut self) -> Result<(), RuntimeError> {
        let elements = self.module.elements.clone();

        // Preflight every active segment before mutating a potentially host-shared table.
        // A later OOB segment must not leave earlier segment writes externally visible.
        for (segment_index, segment) in elements.iter().enumerate() {
            if segment.table_index != 0 {
                return Err(RuntimeError::TableIndexOutOfBounds(segment.table_index));
            }
            let offset = u64::from(segment.offset as u32);
            let end = offset
                .checked_add(segment.function_indices.len() as u64)
                .ok_or(RuntimeError::ElementSegmentOutOfBounds {
                    segment: segment_index,
                    offset,
                    length: segment.function_indices.len(),
                })?;
            let table = self
                .table
                .as_ref()
                .ok_or(RuntimeError::TableIndexOutOfBounds(0))?;
            if end > u64::from(table.len()) {
                return Err(RuntimeError::ElementSegmentOutOfBounds {
                    segment: segment_index,
                    offset,
                    length: segment.function_indices.len(),
                });
            }
        }

        for segment in &elements {
            let offset = u64::from(segment.offset as u32);
            let table = self
                .table
                .as_ref()
                .ok_or(RuntimeError::TableIndexOutOfBounds(0))?;
            for (slot, &function_index) in segment.function_indices.iter().enumerate() {
                let index = u32::try_from(offset + slot as u64).map_err(|_| {
                    RuntimeError::ControlInvariant(
                        "preflighted element segment index no longer fits u32",
                    )
                })?;
                table
                    .set_for_instance(index, function_index, &self.identity)
                    .map_err(|error| map_table_element_error(error, index))?;
            }
        }
        Ok(())
    }
'''
new_runtime_segments = '''    fn initialize_data_segments(&mut self) -> Result<(), RuntimeError> {
        let data = self.module.data.clone();

        // Preflight every active segment before mutating a potentially host-shared memory.
        for (segment_index, segment) in data.iter().enumerate() {
            let DataMode::Active {
                memory_index,
                offset,
            } = segment.mode
            else {
                continue;
            };
            if memory_index != 0 {
                return Err(RuntimeError::MemoryIndexOutOfBounds(memory_index));
            }
            let offset = u64::from(offset as u32);
            let end = offset.checked_add(segment.bytes.len() as u64).ok_or(
                RuntimeError::DataSegmentOutOfBounds {
                    segment: segment_index,
                    offset,
                    length: segment.bytes.len(),
                },
            )?;
            let memory_len = self.with_memory(|memory| Ok(memory.bytes.len() as u64))?;
            if end > memory_len {
                return Err(RuntimeError::DataSegmentOutOfBounds {
                    segment: segment_index,
                    offset,
                    length: segment.bytes.len(),
                });
            }
        }

        for segment in &data {
            let DataMode::Active { offset, .. } = segment.mode else {
                continue;
            };
            let offset = u64::from(offset as u32);
            self.with_memory_mut(|memory| {
                let start = usize::try_from(offset).map_err(|_| {
                    RuntimeError::ControlInvariant("preflighted data offset no longer fits usize")
                })?;
                let end = start + segment.bytes.len();
                memory.bytes[start..end].copy_from_slice(&segment.bytes);
                Ok(())
            })?;
        }
        Ok(())
    }

    fn initialize_element_segments(&mut self) -> Result<(), RuntimeError> {
        let elements = self.module.elements.clone();

        // Preflight every active segment before mutating a potentially host-shared table.
        // A later OOB segment must not leave earlier segment writes externally visible.
        for (segment_index, segment) in elements.iter().enumerate() {
            let ElementMode::Active {
                table_index,
                offset,
            } = segment.mode
            else {
                continue;
            };
            if table_index != 0 {
                return Err(RuntimeError::TableIndexOutOfBounds(table_index));
            }
            let offset = u64::from(offset as u32);
            let end = offset
                .checked_add(segment.function_indices.len() as u64)
                .ok_or(RuntimeError::ElementSegmentOutOfBounds {
                    segment: segment_index,
                    offset,
                    length: segment.function_indices.len(),
                })?;
            let table = self
                .table
                .as_ref()
                .ok_or(RuntimeError::TableIndexOutOfBounds(0))?;
            if end > u64::from(table.len()) {
                return Err(RuntimeError::ElementSegmentOutOfBounds {
                    segment: segment_index,
                    offset,
                    length: segment.function_indices.len(),
                });
            }
        }

        for segment in &elements {
            let ElementMode::Active { offset, .. } = segment.mode else {
                continue;
            };
            let offset = u64::from(offset as u32);
            let table = self
                .table
                .as_ref()
                .ok_or(RuntimeError::TableIndexOutOfBounds(0))?;
            for (slot, &function_index) in segment.function_indices.iter().enumerate() {
                let index = u32::try_from(offset + slot as u64).map_err(|_| {
                    RuntimeError::ControlInvariant(
                        "preflighted element segment index no longer fits u32",
                    )
                })?;
                table
                    .set_for_instance(index, function_index, &self.identity)
                    .map_err(|error| map_table_element_error(error, index))?;
            }
        }
        Ok(())
    }
'''
replace_once(runtime, old_runtime_segments, new_runtime_segments, "runtime segment initialization")

replace_once(
    runtime,
    "        module.data[0].offset = (WASM_PAGE_SIZE - 2) as i32;\n",
    "        module.data[0].mode = DataMode::Active {\n            memory_index: 0,\n            offset: (WASM_PAGE_SIZE - 2) as i32,\n        };\n",
    "runtime OOB data test migration",
)

Path("crates/wasm-runtime/tests/phase5c_segment_modes.rs").write_text(r'''use wasm_parser::{
    DataMode, DataSegment, ElementMode, ElementSegment, FuncType, FunctionBody, Import, ImportDesc,
    Limits, MemoryType, Module, TableType,
};
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, RuntimeError, TableHandle};
use wasm_validator::ValidationError;

fn noop_function_parts() -> (Vec<FuncType>, Vec<u32>, Vec<FunctionBody>) {
    (
        vec![FuncType {
            params: vec![],
            results: vec![],
        }],
        vec![0],
        vec![FunctionBody {
            locals: vec![],
            code: vec![0x0b],
        }],
    )
}

fn memory_import() -> Import {
    Import {
        module: "env".into(),
        name: "mem".into(),
        desc: ImportDesc::Memory(MemoryType {
            limits: Limits {
                min: 1,
                max: Some(1),
            },
        }),
    }
}

fn table_import() -> Import {
    Import {
        module: "env".into(),
        name: "tab".into(),
        desc: ImportDesc::Table(TableType {
            limits: Limits {
                min: 2,
                max: Some(2),
            },
        }),
    }
}

#[test]
fn passive_data_does_not_require_memory() {
    let module = Module {
        data: vec![DataSegment {
            mode: DataMode::Passive,
            bytes: b"kept".to_vec(),
        }],
        ..Module::default()
    };
    Instance::new(module).expect("passive data has no instantiation target");
}

#[test]
fn passive_data_does_not_mutate_imported_memory() {
    let module = Module {
        imports: vec![memory_import()],
        data: vec![DataSegment {
            mode: DataMode::Passive,
            bytes: b"skip".to_vec(),
        }],
        ..Module::default()
    };
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(4, b"host").unwrap();
    let mut hosts = HostRegistry::new();
    hosts.register_memory("env", "mem", memory.clone()).unwrap();
    Instance::with_hosts(module, hosts).expect("passive data must not execute");
    assert_eq!(memory.read(4, 4).unwrap(), b"host");
}

#[test]
fn explicit_active_data_targets_memory_zero() {
    let module = Module {
        imports: vec![memory_import()],
        data: vec![DataSegment {
            mode: DataMode::Active {
                memory_index: 0,
                offset: 7,
            },
            bytes: b"wasm".to_vec(),
        }],
        ..Module::default()
    };
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let mut hosts = HostRegistry::new();
    hosts.register_memory("env", "mem", memory.clone()).unwrap();
    Instance::with_hosts(module, hosts).unwrap();
    assert_eq!(memory.read(7, 4).unwrap(), b"wasm");
}

#[test]
fn active_data_still_validates_memory_index() {
    let module = Module {
        memories: vec![MemoryType {
            limits: Limits {
                min: 1,
                max: Some(1),
            },
        }],
        data: vec![DataSegment {
            mode: DataMode::Active {
                memory_index: 1,
                offset: 0,
            },
            bytes: vec![1],
        }],
        ..Module::default()
    };
    assert!(matches!(
        Instance::new(module),
        Err(RuntimeError::Validation(ValidationError::DataMemoryOutOfBounds {
            memory_index: 1,
            ..
        }))
    ));
}

#[test]
fn passive_and_declarative_elements_do_not_require_table() {
    let (types, function_type_indices, code) = noop_function_parts();
    let module = Module {
        types,
        function_type_indices,
        code,
        elements: vec![
            ElementSegment {
                mode: ElementMode::Passive,
                function_indices: vec![0],
            },
            ElementSegment {
                mode: ElementMode::Declarative,
                function_indices: vec![0],
            },
        ],
        ..Module::default()
    };
    Instance::new(module).expect("non-active elements have no table target");
}

#[test]
fn passive_and_declarative_elements_do_not_mutate_imported_table() {
    let (types, function_type_indices, code) = noop_function_parts();
    let module = Module {
        types,
        imports: vec![table_import()],
        function_type_indices,
        code,
        elements: vec![
            ElementSegment {
                mode: ElementMode::Passive,
                function_indices: vec![0],
            },
            ElementSegment {
                mode: ElementMode::Declarative,
                function_indices: vec![0],
            },
        ],
        ..Module::default()
    };
    let table = TableHandle::new(2, Some(2)).unwrap();
    let mut hosts = HostRegistry::new();
    hosts.register_table("env", "tab", table.clone()).unwrap();
    Instance::with_hosts(module, hosts).unwrap();
    assert!(table.get(0).unwrap().is_none());
    assert!(table.get(1).unwrap().is_none());
}

#[test]
fn explicit_active_element_targets_table_zero() {
    let (types, function_type_indices, code) = noop_function_parts();
    let module = Module {
        types,
        imports: vec![table_import()],
        function_type_indices,
        code,
        elements: vec![ElementSegment {
            mode: ElementMode::Active {
                table_index: 0,
                offset: 1,
            },
            function_indices: vec![0],
        }],
        ..Module::default()
    };
    let table = TableHandle::new(2, Some(2)).unwrap();
    let mut hosts = HostRegistry::new();
    hosts.register_table("env", "tab", table.clone()).unwrap();
    Instance::with_hosts(module, hosts).unwrap();
    assert!(table.get(0).unwrap().is_none());
    assert!(table.get(1).unwrap().is_some());
}

#[test]
fn passive_element_still_validates_function_indices() {
    let (types, function_type_indices, code) = noop_function_parts();
    let module = Module {
        types,
        function_type_indices,
        code,
        elements: vec![ElementSegment {
            mode: ElementMode::Passive,
            function_indices: vec![1],
        }],
        ..Module::default()
    };
    assert!(matches!(
        Instance::new(module),
        Err(RuntimeError::Validation(
            ValidationError::ElementFunctionOutOfBounds {
                function_index: 1,
                ..
            }
        ))
    ));
}
''')
