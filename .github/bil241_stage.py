from pathlib import Path

runtime = Path('crates/wasm-runtime/src/lib.rs')
s = runtime.read_text()

old_guard = '''        if table_index != 0 {
            return Err(RuntimeError::TableElementOutOfBounds(table_index));
        }
        let width = length as u32 as usize;'''
if s.count(old_guard) != 2:
    raise SystemExit(f'expected 2 table_init/fill legacy guards, found {s.count(old_guard)}')
s = s.replace(old_guard, '        let width = length as u32 as usize;', 2)

old_lookup = '''        let table = self
            .table
            .as_ref()
            .ok_or(RuntimeError::TableElementOutOfBounds(destination as u32))?;'''
new_lookup = '''        let table = self
            .tables
            .get(table_index as usize)
            .ok_or(RuntimeError::TableIndexOutOfBounds(table_index))?;'''
if s.count(old_lookup) != 3:
    raise SystemExit(f'expected 3 legacy destination lookups, found {s.count(old_lookup)}')
s = s.replace(old_lookup, new_lookup, 1)

start = s.index('    fn table_copy(\n')
end = s.index('    fn table_fill(\n', start)
new_copy = '''    fn table_copy(
        &mut self,
        destination_table: u32,
        source_table: u32,
        destination: i32,
        source: i32,
        length: i32,
    ) -> Result<(), RuntimeError> {
        let width = length as u32 as usize;
        let source_start = u64::from(source as u32);
        let destination_start = u64::from(destination as u32);
        let source_handle = self
            .tables
            .get(source_table as usize)
            .cloned()
            .ok_or(RuntimeError::TableIndexOutOfBounds(source_table))?;
        let destination_handle = self
            .tables
            .get(destination_table as usize)
            .cloned()
            .ok_or(RuntimeError::TableIndexOutOfBounds(destination_table))?;
        let source_end = source_start
            .checked_add(width as u64)
            .ok_or(RuntimeError::TableElementOutOfBounds(source as u32))?;
        let destination_end = destination_start
            .checked_add(width as u64)
            .ok_or(RuntimeError::TableElementOutOfBounds(destination as u32))?;
        if source_end > u64::from(source_handle.len()) {
            return Err(RuntimeError::TableElementOutOfBounds(source as u32));
        }
        if destination_end > u64::from(destination_handle.len()) {
            return Err(RuntimeError::TableElementOutOfBounds(destination as u32));
        }
        let source_start = source_start as usize;
        let destination_start = destination_start as usize;
        let copied = source_handle.slots.borrow()[source_start..source_start + width].to_vec();
        destination_handle.slots.borrow_mut()
            [destination_start..destination_start + width]
            .clone_from_slice(&copied);
        Ok(())
    }

'''
s = s[:start] + new_copy + s[end:]
if s.count(old_lookup) != 1:
    raise SystemExit(f'expected one legacy fill lookup after copy rewrite, found {s.count(old_lookup)}')
s = s.replace(old_lookup, new_lookup, 1)

old_size = '''    fn table_size(&self, table_index: u32) -> Result<i32, RuntimeError> {
        if table_index != 0 {
            return Err(RuntimeError::TableElementOutOfBounds(table_index));
        }
        let table = self
            .table
            .as_ref()
            .ok_or(RuntimeError::TableElementOutOfBounds(table_index))?;
        Ok(table.len() as i32)
    }
'''
new_size = '''    fn table_size(&self, table_index: u32) -> Result<i32, RuntimeError> {
        let table = self
            .tables
            .get(table_index as usize)
            .ok_or(RuntimeError::TableIndexOutOfBounds(table_index))?;
        Ok(table.len() as i32)
    }
'''
if old_size not in s:
    raise SystemExit('table_size legacy body not found')
s = s.replace(old_size, new_size, 1)

old_grow_guard = '''                            if table_index != 0 || self.table.is_none() {
                                return Err(RuntimeError::TableIndexOutOfBounds(table_index));
                            }
'''
if s.count(old_grow_guard) != 1:
    raise SystemExit(f'expected one table.grow guard, found {s.count(old_grow_guard)}')
s = s.replace(old_grow_guard, '', 1)
old_grow_lookup = '''                            let previous = self
                                .table
                                .as_ref()
                                .ok_or(RuntimeError::TableIndexOutOfBounds(table_index))?
                                .grow(delta, fill);'''
new_grow_lookup = '''                            let previous = self
                                .tables
                                .get(table_index as usize)
                                .ok_or(RuntimeError::TableIndexOutOfBounds(table_index))?
                                .grow(delta, fill);'''
if old_grow_lookup not in s:
    raise SystemExit('table.grow lookup not found')
s = s.replace(old_grow_lookup, new_grow_lookup, 1)
runtime.write_text(s)

typed = Path('crates/wasm-validator/src/typed.rs')
t = typed.read_text()
old_dest = '''                        if destination_table != 0
                            || destination_table as usize >= module.table_count()
                        {'''
if old_dest not in t:
    raise SystemExit('destination table.copy validator guard not found')
t = t.replace(old_dest, '                        if destination_table as usize >= module.table_count() {', 1)
old_src = '                        if source_table != 0 || source_table as usize >= module.table_count() {'
if old_src not in t:
    raise SystemExit('source table.copy validator guard not found')
t = t.replace(old_src, '                        if source_table as usize >= module.table_count() {', 1)
old_idx = '                        if table_index != 0 || table_index as usize >= module.table_count() {'
if t.count(old_idx) != 3:
    raise SystemExit(f'expected 3 table.grow/size/fill guards, found {t.count(old_idx)}')
t = t.replace(old_idx, '                        if table_index as usize >= module.table_count() {', 3)
typed.write_text(t)

tests = Path('crates/wasm-runtime/tests/multi_table_bulk.rs')
tests.write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};

fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

fn two_table_module(body: &[u8], element_payload: Option<&[u8]>, table_payload: &[u8]) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 0, 1, 0x7f]);
    section(&mut module, 3, &[2, 0, 0]);
    section(&mut module, 4, table_payload);
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 1]);
    if let Some(element) = element_payload {
        section(&mut module, 9, element);
    }
    let mut code = vec![2, 4, 0, 0x41, 42, 0x0b, (body.len() + 1) as u8, 0];
    code.extend_from_slice(body);
    section(&mut module, 10, &code);
    module
}

#[test]
fn table_copy_crosses_distinct_tables() {
    let tables = [2, 0x70, 1, 1, 1, 0x70, 1, 1, 1];
    let element = [1, 2, 1, 0x41, 0, 0x0b, 0, 1, 0];
    let body = [0x41, 0, 0x41, 0, 0x41, 1, 0xfc, 14, 0, 1, 0x41, 0, 0x11, 0, 0, 0x0b];
    let module = parse_module(&two_table_module(&body, Some(&element), &tables)).unwrap();
    let mut instance = Instance::new(module).unwrap();
    assert_eq!(instance.invoke_export("run", &[]).unwrap(), Some(Value::I32(42)));
}

#[test]
fn table_grow_and_size_use_second_table() {
    let tables = [2, 0x70, 1, 1, 1, 0x70, 1, 1, 3];
    let body = [0xd0, 0x70, 0x41, 1, 0xfc, 15, 1, 0x1a, 0xfc, 16, 1, 0x0b];
    let module = parse_module(&two_table_module(&body, None, &tables)).unwrap();
    let mut instance = Instance::new(module).unwrap();
    assert_eq!(instance.invoke_export("run", &[]).unwrap(), Some(Value::I32(2)));
}

#[test]
fn table_fill_targets_second_table() {
    let tables = [2, 0x70, 1, 1, 1, 0x70, 1, 2, 2];
    let element = [1, 2, 1, 0x41, 0, 0x0b, 0, 2, 0, 0];
    let body = [0x41, 0, 0xd0, 0x70, 0x41, 2, 0xfc, 17, 1, 0x41, 1, 0x25, 1, 0xd1, 0x0b];
    let module = parse_module(&two_table_module(&body, Some(&element), &tables)).unwrap();
    let mut instance = Instance::new(module).unwrap();
    assert_eq!(instance.invoke_export("run", &[]).unwrap(), Some(Value::I32(1)));
}

#[test]
fn table_init_targets_second_table() {
    let tables = [2, 0x70, 1, 1, 1, 0x70, 1, 1, 1];
    let element = [1, 1, 0, 1, 0];
    let body = [0x41, 0, 0x41, 0, 0x41, 1, 0xfc, 12, 0, 1, 0x41, 0, 0x11, 0, 1, 0x0b];
    let module = parse_module(&two_table_module(&body, Some(&element), &tables)).unwrap();
    let mut instance = Instance::new(module).unwrap();
    assert_eq!(instance.invoke_export("run", &[]).unwrap(), Some(Value::I32(42)));
}
''')
