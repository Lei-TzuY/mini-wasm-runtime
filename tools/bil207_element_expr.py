from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    if text.count(old) != 1:
        raise SystemExit(f"expected exactly one match in {path}, found {text.count(old)}")
    file.write_text(text.replace(old, new, 1))


replace_once(
    "crates/wasm-parser/src/lib.rs",
    "#[derive(Debug, Clone, PartialEq, Eq)]\npub struct ElementSegment {\n    pub mode: ElementMode,\n    pub function_indices: Vec<u32>,\n}\n",
    "pub const NULL_FUNCREF_INDEX: u32 = u32::MAX;\n\n#[derive(Debug, Clone, PartialEq, Eq)]\npub struct ElementSegment {\n    pub mode: ElementMode,\n    /// Function indices for legacy/ref.func items; NULL_FUNCREF_INDEX encodes ref.null.\n    pub function_indices: Vec<u32>,\n}\n",
)

old_element_parser = '''fn parse_element_section(cursor: &mut Cursor<'_>, module: &mut Module) -> Result<(), ParseError> {
    let count = cursor.read_u32()?;
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
        let mut function_indices = Vec::new();
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
new_element_parser = '''fn parse_element_section(cursor: &mut Cursor<'_>, module: &mut Module) -> Result<(), ParseError> {
    let count = cursor.read_u32()?;
    for _ in 0..count {
        let flags = cursor.read_u32()?;
        let (mode, expressions) = match flags {
            0 => (
                ElementMode::Active {
                    table_index: 0,
                    offset: read_i32_const_expr(cursor)?,
                },
                false,
            ),
            1 => {
                read_legacy_element_kind(cursor)?;
                (ElementMode::Passive, false)
            }
            2 => {
                let table_index = cursor.read_u32()?;
                let offset = read_i32_const_expr(cursor)?;
                read_legacy_element_kind(cursor)?;
                (
                    ElementMode::Active {
                        table_index,
                        offset,
                    },
                    false,
                )
            }
            3 => {
                read_legacy_element_kind(cursor)?;
                (ElementMode::Declarative, false)
            }
            4 => (
                ElementMode::Active {
                    table_index: 0,
                    offset: read_i32_const_expr(cursor)?,
                },
                true,
            ),
            5 => {
                read_element_reference_type(cursor)?;
                (ElementMode::Passive, true)
            }
            6 => {
                let table_index = cursor.read_u32()?;
                let offset = read_i32_const_expr(cursor)?;
                read_element_reference_type(cursor)?;
                (
                    ElementMode::Active {
                        table_index,
                        offset,
                    },
                    true,
                )
            }
            7 => {
                read_element_reference_type(cursor)?;
                (ElementMode::Declarative, true)
            }
            other => return Err(ParseError::UnsupportedElementSegmentMode(other)),
        };
        let item_count = cursor.read_u32()?;
        let mut function_indices = Vec::new();
        for _ in 0..item_count {
            function_indices.push(if expressions {
                read_element_reference_expr(cursor)?
            } else {
                cursor.read_u32()?
            });
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

fn read_element_reference_type(cursor: &mut Cursor<'_>) -> Result<(), ParseError> {
    let reference_type = cursor.read_u8()?;
    if reference_type == 0x70 {
        Ok(())
    } else {
        Err(ParseError::InvalidReferenceType(reference_type))
    }
}

fn read_element_reference_expr(cursor: &mut Cursor<'_>) -> Result<u32, ParseError> {
    match read_const_expr(cursor)? {
        Constant::FuncRef(Some(function_index)) => Ok(function_index),
        Constant::FuncRef(None) => Ok(NULL_FUNCREF_INDEX),
        other => Err(ParseError::ConstExprTypeMismatch {
            expected: ValueType::FuncRef,
            actual: other.value_type(),
        }),
    }
}
'''
replace_once("crates/wasm-parser/src/lib.rs", old_element_parser, new_element_parser)

replace_once(
    "crates/wasm-validator/src/phase5.rs",
    "use wasm_parser::{ElementMode, Module};\n",
    "use wasm_parser::{ElementMode, Module, NULL_FUNCREF_INDEX};\n",
)
replace_once(
    "crates/wasm-validator/src/phase5.rs",
    "        for &function_index in &element.function_indices {\n            if function_index as usize >= total_functions {\n",
    "        for &function_index in &element.function_indices {\n            if function_index == NULL_FUNCREF_INDEX {\n                continue;\n            }\n            if function_index as usize >= total_functions {\n",
)

replace_once(
    "crates/wasm-runtime/src/lib.rs",
    "    FuncType, ImportDesc, ImportKind, Module, ParseError, ValueType,\n};\n",
    "    FuncType, ImportDesc, ImportKind, Module, ParseError, ValueType, NULL_FUNCREF_INDEX,\n};\n",
)
replace_once(
    "crates/wasm-runtime/src/lib.rs",
    "        for (offset, function_index) in functions.into_iter().enumerate() {\n            slots[destination_start + offset] = Some(FunctionRef {\n                owner: Rc::downgrade(&self.identity),\n                function_index,\n            });\n        }\n",
    "        for (offset, function_index) in functions.into_iter().enumerate() {\n            slots[destination_start + offset] = if function_index == NULL_FUNCREF_INDEX {\n                None\n            } else {\n                Some(FunctionRef {\n                    owner: Rc::downgrade(&self.identity),\n                    function_index,\n                })\n            };\n        }\n",
)
replace_once(
    "crates/wasm-runtime/src/lib.rs",
    "            for (slot, &function_index) in segment.function_indices.iter().enumerate() {\n                let index = u32::try_from(offset + slot as u64).map_err(|_| {\n                    RuntimeError::ControlInvariant(\n                        \"preflighted element segment index no longer fits u32\",\n                    )\n                })?;\n                table\n                    .set_for_instance(index, function_index, &self.identity)\n                    .map_err(|error| map_table_element_error(error, index))?;\n            }\n",
    "            for (slot, &function_index) in segment.function_indices.iter().enumerate() {\n                let index = u32::try_from(offset + slot as u64).map_err(|_| {\n                    RuntimeError::ControlInvariant(\n                        \"preflighted element segment index no longer fits u32\",\n                    )\n                })?;\n                if function_index == NULL_FUNCREF_INDEX {\n                    table\n                        .set(index, None)\n                        .map_err(|error| map_table_element_error(error, index))?;\n                } else {\n                    table\n                        .set_for_instance(index, function_index, &self.identity)\n                        .map_err(|error| map_table_element_error(error, index))?;\n                }\n            }\n",
)

test = r'''use wasm_parser::{parse_module, ElementMode, NULL_FUNCREF_INDEX};
use wasm_runtime::{Instance, RuntimeError, Value};

fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

fn base_module(element_payload: &[u8], run_ops: &[u8]) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 0, 1, 0x7f]);
    section(&mut module, 3, &[2, 0, 0]);
    section(&mut module, 4, &[1, 0x70, 0, 1]);
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 1]);
    section(&mut module, 9, element_payload);

    let target = [0, 0x41, 42, 0x0b];
    let mut run = vec![0];
    run.extend_from_slice(run_ops);
    run.push(0x0b);
    let mut code = vec![2, target.len() as u8];
    code.extend_from_slice(&target);
    code.push(run.len() as u8);
    code.extend_from_slice(&run);
    section(&mut module, 10, &code);
    module
}

fn call_indirect_slot_zero() -> Vec<u8> {
    vec![0x41, 0, 0x11, 0, 0]
}

#[test]
fn active_expression_ref_func_dispatches_through_table() {
    // flags=4, active table 0, offset i32.const 0, one (ref.func 0) expression.
    let element = [1, 4, 0x41, 0, 0x0b, 1, 0xd2, 0, 0x0b];
    let module = parse_module(&base_module(&element, &call_indirect_slot_zero())).unwrap();
    assert!(matches!(
        module.elements[0].mode,
        ElementMode::Active {
            table_index: 0,
            offset: 0
        }
    ));
    let mut instance = Instance::new(module).unwrap();
    assert_eq!(instance.invoke_export("run", &[]).unwrap(), Some(Value::I32(42)));
}

#[test]
fn active_expression_ref_null_leaves_slot_uninitialized() {
    let element = [1, 4, 0x41, 0, 0x0b, 1, 0xd0, 0x70, 0x0b];
    let module = parse_module(&base_module(&element, &call_indirect_slot_zero())).unwrap();
    assert_eq!(module.elements[0].function_indices, vec![NULL_FUNCREF_INDEX]);
    let mut instance = Instance::new(module).unwrap();
    assert!(matches!(
        instance.invoke_export("run", &[]),
        Err(RuntimeError::UninitializedTableElement(0))
    ));
}

#[test]
fn passive_expression_segment_feeds_table_init() {
    // flags=5, funcref, one ref.func expression.
    let element = [1, 5, 0x70, 1, 0xd2, 0, 0x0b];
    let run = [
        0x41, 0, // destination
        0x41, 0, // source
        0x41, 1, // length
        0xfc, 0x0c, 0, 0, // table.init element 0 table 0
        0x41, 0, 0x11, 0, 0, // call_indirect type 0 table 0
    ];
    let module = parse_module(&base_module(&element, &run)).unwrap();
    let mut instance = Instance::new(module).unwrap();
    assert_eq!(instance.invoke_export("run", &[]).unwrap(), Some(Value::I32(42)));
}

#[test]
fn explicit_table_and_declarative_expression_modes_parse() {
    let active = [1, 6, 0, 0x41, 0, 0x0b, 0x70, 1, 0xd2, 0, 0x0b];
    let module = parse_module(&base_module(&active, &call_indirect_slot_zero())).unwrap();
    assert!(matches!(module.elements[0].mode, ElementMode::Active { table_index: 0, .. }));

    let declarative = [1, 7, 0x70, 1, 0xd2, 0, 0x0b];
    let module = parse_module(&base_module(&declarative, &[0x41, 7])).unwrap();
    assert!(matches!(module.elements[0].mode, ElementMode::Declarative));
}

#[test]
fn expression_segment_rejects_non_funcref_constant() {
    let element = [1, 4, 0x41, 0, 0x0b, 1, 0x41, 0, 0x0b];
    assert!(parse_module(&base_module(&element, &[0x41, 0])).is_err());
}
'''
Path("crates/wasm-runtime/tests/element_expr_segments.rs").write_text(test)
