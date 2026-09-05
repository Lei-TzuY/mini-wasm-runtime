from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    if text.count(old) != 1:
        raise SystemExit(f"expected exactly one anchor in {path}, found {text.count(old)}")
    p.write_text(text.replace(old, new, 1))


runtime = "crates/wasm-runtime/src/lib.rs"
validator = "crates/wasm-validator/src/typed.rs"

replace_once(
    runtime,
    """    fn elem_drop(&mut self, element_index: u32) -> Result<(), RuntimeError> {\n""",
    """    fn table_size(&self, table_index: u32) -> Result<i32, RuntimeError> {\n        if table_index != 0 {\n            return Err(RuntimeError::TableElementOutOfBounds(table_index));\n        }\n        let table = self\n            .table\n            .as_ref()\n            .ok_or(RuntimeError::TableElementOutOfBounds(table_index))?;\n        Ok(table.len() as i32)\n    }\n\n    fn elem_drop(&mut self, element_index: u32) -> Result<(), RuntimeError> {\n""",
)

replace_once(
    runtime,
    """                        14 => {\n                            let destination_table = read_u32_immediate(code, &mut pc)?;\n                            let source_table = read_u32_immediate(code, &mut pc)?;\n                            let length = numeric::i32_from_stack(&mut stack)?;\n                            let source = numeric::i32_from_stack(&mut stack)?;\n                            let destination = numeric::i32_from_stack(&mut stack)?;\n                            self.table_copy(\n                                destination_table,\n                                source_table,\n                                destination,\n                                source,\n                                length,\n                            )?;\n                        }\n                        _ => {\n""",
    """                        14 => {\n                            let destination_table = read_u32_immediate(code, &mut pc)?;\n                            let source_table = read_u32_immediate(code, &mut pc)?;\n                            let length = numeric::i32_from_stack(&mut stack)?;\n                            let source = numeric::i32_from_stack(&mut stack)?;\n                            let destination = numeric::i32_from_stack(&mut stack)?;\n                            self.table_copy(\n                                destination_table,\n                                source_table,\n                                destination,\n                                source,\n                                length,\n                            )?;\n                        }\n                        16 => {\n                            let table_index = read_u32_immediate(code, &mut pc)?;\n                            stack.push(Value::I32(self.table_size(table_index)?));\n                        }\n                        _ => {\n""",
)

replace_once(
    runtime,
    """                    14 => {\n                        let _ = read_u32_immediate(code, &mut pc)?;\n                        let _ = read_u32_immediate(code, &mut pc)?;\n                    }\n                    _ => {\n""",
    """                    14 => {\n                        let _ = read_u32_immediate(code, &mut pc)?;\n                        let _ = read_u32_immediate(code, &mut pc)?;\n                    }\n                    16 => {\n                        let _ = read_u32_immediate(code, &mut pc)?;\n                    }\n                    _ => {\n""",
)

replace_once(
    validator,
    """                    14 => {\n                        let destination_table = read_u32(code, &mut pc, function, offset)?;\n                        if destination_table != 0\n                            || destination_table as usize >= module.table_count()\n                        {\n                            return Err(ValidationError::TableIndexOutOfBounds {\n                                function,\n                                offset,\n                                table_index: destination_table,\n                            });\n                        }\n                        let source_table = read_u32(code, &mut pc, function, offset)?;\n                        if source_table != 0 || source_table as usize >= module.table_count() {\n                            return Err(ValidationError::TableIndexOutOfBounds {\n                                function,\n                                offset,\n                                table_index: source_table,\n                            });\n                        }\n                        pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;\n                        pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;\n                        pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;\n                    }\n                    _ => {\n""",
    """                    14 => {\n                        let destination_table = read_u32(code, &mut pc, function, offset)?;\n                        if destination_table != 0\n                            || destination_table as usize >= module.table_count()\n                        {\n                            return Err(ValidationError::TableIndexOutOfBounds {\n                                function,\n                                offset,\n                                table_index: destination_table,\n                            });\n                        }\n                        let source_table = read_u32(code, &mut pc, function, offset)?;\n                        if source_table != 0 || source_table as usize >= module.table_count() {\n                            return Err(ValidationError::TableIndexOutOfBounds {\n                                function,\n                                offset,\n                                table_index: source_table,\n                            });\n                        }\n                        pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;\n                        pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;\n                        pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;\n                    }\n                    16 => {\n                        let table_index = read_u32(code, &mut pc, function, offset)?;\n                        if table_index != 0 || table_index as usize >= module.table_count() {\n                            return Err(ValidationError::TableIndexOutOfBounds {\n                                function,\n                                offset,\n                                table_index,\n                            });\n                        }\n                        stack.push(ValueType::I32);\n                    }\n                    _ => {\n""",
)

Path("crates/wasm-runtime/tests/bulk_table_size.rs").write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, TableHandle, Value};
use wasm_validator::ValidationError;

fn u32leb(out: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn name(out: &mut Vec<u8>, value: &str) {
    u32leb(out, value.len() as u32);
    out.extend_from_slice(value.as_bytes());
}

fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    u32leb(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn module(table_index: u32, minimum: u8) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 0, 1, 0x7f]);

    let mut imports = vec![1];
    name(&mut imports, "env");
    name(&mut imports, "tab");
    imports.extend([1, 0x70, 0, minimum]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[1, 0]);
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);

    let mut body = vec![0, 0xfc, 16];
    u32leb(&mut body, table_index);
    body.push(0x0b);
    let mut code = vec![1];
    u32leb(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    section(&mut module, 10, &code);
    module
}

fn hosts(table: &TableHandle) -> HostRegistry {
    let mut hosts = HostRegistry::new();
    hosts.register_table("env", "tab", table.clone()).unwrap();
    hosts
}

#[test]
fn reports_imported_table_size() {
    let table = TableHandle::new(4, Some(8)).unwrap();
    let mut vm = Instance::with_hosts(parse_module(&module(0, 4)).unwrap(), hosts(&table)).unwrap();
    assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(4)));
}

#[test]
fn reports_zero_length_table() {
    let table = TableHandle::new(0, Some(8)).unwrap();
    let mut vm = Instance::with_hosts(parse_module(&module(0, 0)).unwrap(), hosts(&table)).unwrap();
    assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(0)));
}

#[test]
fn rejects_nonzero_table_index() {
    let table = TableHandle::new(4, Some(8)).unwrap();
    assert!(matches!(
        Instance::with_hosts(parse_module(&module(1, 4)).unwrap(), hosts(&table)),
        Err(wasm_runtime::RuntimeError::Validation(
            ValidationError::TableIndexOutOfBounds { table_index: 1, .. }
        ))
    ));
}
''')

Path("docs/bulk-table-size.md").write_text('''# Bulk table: `table.size`\n\nAdds executable `table.size` (`0xfc 16`) for the current single `funcref` table surface. Validation accepts table index 0 only and pushes an `i32`; runtime reports the live `TableHandle` length, including imported tables, and fails closed on unsupported non-zero table indices.\n\n`table.grow` and `table.fill` remain out of scope for this slice because the operand stack does not yet carry reference values; they should land with an explicit reference-value model rather than a numeric-stack shortcut.\n''')
