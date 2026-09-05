from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected exactly one anchor in {path}, found {count}")
    p.write_text(text.replace(old, new, 1))


runtime = "crates/wasm-runtime/src/lib.rs"
method_anchor = "    fn elem_drop(&mut self, element_index: u32) -> Result<(), RuntimeError> {"
method = '''    fn table_copy(
        &mut self,
        destination_table: u32,
        source_table: u32,
        destination: i32,
        source: i32,
        length: i32,
    ) -> Result<(), RuntimeError> {
        if destination_table != 0 {
            return Err(RuntimeError::TableElementOutOfBounds(destination_table));
        }
        if source_table != 0 {
            return Err(RuntimeError::TableElementOutOfBounds(source_table));
        }
        let width = length as u32 as usize;
        let source_start = u64::from(source as u32);
        let destination_start = u64::from(destination as u32);
        let table = self
            .table
            .as_ref()
            .ok_or(RuntimeError::TableElementOutOfBounds(destination as u32))?;
        let table_len = table.slots.borrow().len() as u64;
        let source_end = source_start
            .checked_add(width as u64)
            .ok_or(RuntimeError::TableElementOutOfBounds(source as u32))?;
        let destination_end = destination_start
            .checked_add(width as u64)
            .ok_or(RuntimeError::TableElementOutOfBounds(destination as u32))?;
        if source_end > table_len {
            return Err(RuntimeError::TableElementOutOfBounds(source as u32));
        }
        if destination_end > table_len {
            return Err(RuntimeError::TableElementOutOfBounds(destination as u32));
        }
        let source_start = source_start as usize;
        let destination_start = destination_start as usize;
        let mut slots = table.slots.borrow_mut();
        let copied = slots[source_start..source_start + width].to_vec();
        slots[destination_start..destination_start + width].clone_from_slice(&copied);
        Ok(())
    }

'''
replace_once(runtime, method_anchor, method + method_anchor)

opcode_anchor = '''                        13 => {
                            let element_index = read_u32_immediate(code, &mut pc)?;
                            self.elem_drop(element_index)?;
                        }
'''
opcode_insert = opcode_anchor + '''                        14 => {
                            let destination_table = read_u32_immediate(code, &mut pc)?;
                            let source_table = read_u32_immediate(code, &mut pc)?;
                            let length = numeric::i32_from_stack(&mut stack)?;
                            let source = numeric::i32_from_stack(&mut stack)?;
                            let destination = numeric::i32_from_stack(&mut stack)?;
                            self.table_copy(
                                destination_table,
                                source_table,
                                destination,
                                source,
                                length,
                            )?;
                        }
'''
replace_once(runtime, opcode_anchor, opcode_insert)

control_anchor = '''                    13 => {
                        let _ = read_u32_immediate(code, &mut pc)?;
                    }
'''
control_insert = control_anchor + '''                    14 => {
                        let _ = read_u32_immediate(code, &mut pc)?;
                        let _ = read_u32_immediate(code, &mut pc)?;
                    }
'''
replace_once(runtime, control_anchor, control_insert)

validator = "crates/wasm-validator/src/typed.rs"
validator_anchor = '''                    13 => {
                        let element_index = read_u32(code, &mut pc, function, offset)?;
                        if element_index as usize >= module.elements.len() {
                            return Err(ValidationError::ElementIndexOutOfBounds {
                                function,
                                offset,
                                element_index,
                            });
                        }
                    }
'''
validator_insert = validator_anchor + '''                    14 => {
                        let destination_table = read_u32(code, &mut pc, function, offset)?;
                        if destination_table != 0 || destination_table as usize >= module.table_count() {
                            return Err(ValidationError::TableIndexOutOfBounds {
                                function,
                                offset,
                                table_index: destination_table,
                            });
                        }
                        let source_table = read_u32(code, &mut pc, function, offset)?;
                        if source_table != 0 || source_table as usize >= module.table_count() {
                            return Err(ValidationError::TableIndexOutOfBounds {
                                function,
                                offset,
                                table_index: source_table,
                            });
                        }
                        pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                    }
'''
replace_once(validator, validator_anchor, validator_insert)

tests = r'''use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, RuntimeError, TableHandle};
use wasm_validator::ValidationError;

fn u32leb(out: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 { byte |= 0x80; }
        out.push(byte);
        if value == 0 { break; }
    }
}
fn name(out: &mut Vec<u8>, value: &str) { u32leb(out, value.len() as u32); out.extend_from_slice(value.as_bytes()); }
fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) { module.push(id); u32leb(module, payload.len() as u32); module.extend_from_slice(payload); }
fn module(body: &[u8]) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 0, 0]);
    let mut imports = vec![1];
    name(&mut imports, "env"); name(&mut imports, "tab"); imports.extend([1, 0x70, 0, 4]);
    section(&mut module, 2, &imports);
    section(&mut module, 3, &[1, 0]);
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);
    section(&mut module, 9, &[1, 1, 0, 1, 0]);
    let mut code = vec![1, (body.len() + 1) as u8, 0]; code.extend_from_slice(body); section(&mut module, 10, &code);
    module
}
fn hosts(table: &TableHandle) -> HostRegistry { let mut hosts = HostRegistry::new(); hosts.register_table("env", "tab", table.clone()).unwrap(); hosts }
fn init(destination: u8, body: &mut Vec<u8>) { body.extend([0x41, destination, 0x41, 0, 0x41, 1, 0xfc, 12, 0, 0]); }
fn copy(destination: u8, source: u8, length: u8, body: &mut Vec<u8>) { body.extend([0x41, destination, 0x41, source, 0x41, length, 0xfc, 14, 0, 0]); }
fn present(table: &TableHandle) -> Vec<bool> { (0..table.len()).map(|i| table.get(i).unwrap().is_some()).collect() }

#[test]
fn forward_overlap_is_memmove_safe() {
    let mut body = Vec::new(); init(0, &mut body); init(2, &mut body); copy(1, 0, 3, &mut body); body.push(0x0b);
    let table = TableHandle::new(4, Some(4)).unwrap();
    let mut vm = Instance::with_hosts(parse_module(&module(&body)).unwrap(), hosts(&table)).unwrap();
    vm.invoke_export("run", &[]).unwrap();
    assert_eq!(present(&table), vec![true, true, false, true]);
}
#[test]
fn backward_overlap_is_memmove_safe() {
    let mut body = Vec::new(); init(1, &mut body); init(3, &mut body); copy(0, 1, 3, &mut body); body.push(0x0b);
    let table = TableHandle::new(4, Some(4)).unwrap();
    let mut vm = Instance::with_hosts(parse_module(&module(&body)).unwrap(), hosts(&table)).unwrap();
    vm.invoke_export("run", &[]).unwrap();
    assert_eq!(present(&table), vec![true, false, true, true]);
}
#[test]
fn destination_oob_traps_atomically() {
    let mut body = Vec::new(); init(0, &mut body); copy(3, 0, 2, &mut body); body.push(0x0b);
    let table = TableHandle::new(4, Some(4)).unwrap();
    let mut vm = Instance::with_hosts(parse_module(&module(&body)).unwrap(), hosts(&table)).unwrap();
    assert!(matches!(vm.invoke_export("run", &[]), Err(RuntimeError::TableElementOutOfBounds(_))));
    assert_eq!(present(&table), vec![true, false, false, false]);
}
#[test]
fn source_oob_traps_atomically() {
    let mut body = Vec::new(); init(0, &mut body); copy(1, 3, 2, &mut body); body.push(0x0b);
    let table = TableHandle::new(4, Some(4)).unwrap();
    let mut vm = Instance::with_hosts(parse_module(&module(&body)).unwrap(), hosts(&table)).unwrap();
    assert!(matches!(vm.invoke_export("run", &[]), Err(RuntimeError::TableElementOutOfBounds(_))));
    assert_eq!(present(&table), vec![true, false, false, false]);
}
#[test]
fn rejects_nonzero_destination_table() {
    let body = [0x41,0,0x41,0,0x41,0,0xfc,14,1,0,0x0b];
    let table = TableHandle::new(4, Some(4)).unwrap();
    assert!(matches!(Instance::with_hosts(parse_module(&module(&body)).unwrap(), hosts(&table)), Err(RuntimeError::Validation(ValidationError::TableIndexOutOfBounds { table_index: 1, .. }))));
}
#[test]
fn rejects_nonzero_source_table() {
    let body = [0x41,0,0x41,0,0x41,0,0xfc,14,0,1,0x0b];
    let table = TableHandle::new(4, Some(4)).unwrap();
    assert!(matches!(Instance::with_hosts(parse_module(&module(&body)).unwrap(), hosts(&table)), Err(RuntimeError::Validation(ValidationError::TableIndexOutOfBounds { table_index: 1, .. }))));
}
'''
Path("crates/wasm-runtime/tests/bulk_table_copy.rs").write_text(tests)
Path("docs/bulk-table-copy.md").write_text("""# Bulk table: `table.copy`\n\nAdds executable `table.copy` (`0xfc 14`) for the current single `funcref` table surface. Validation requires three i32 operands and fails closed on non-zero table indices. Runtime preflights source and destination ranges before mutation and snapshots the source range, preserving overlap-safe memmove semantics while mutating imported `TableHandle` backing directly.\n""")
