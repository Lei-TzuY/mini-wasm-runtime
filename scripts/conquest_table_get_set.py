from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    if text.count(old) != 1:
        raise SystemExit(f"expected exactly one anchor in {path}, found {text.count(old)}")
    p.write_text(text.replace(old, new, 1))


validator = "crates/wasm-validator/src/typed.rs"
old = '''            0x28..=0x35 => {
'''
new = '''            0x25 => {
                let table_index = read_u32(code, &mut pc, function, offset)?;
                if table_index != 0 || table_index as usize >= module.table_count() {
                    return Err(ValidationError::TableIndexOutOfBounds {
                        function,
                        offset,
                        table_index,
                    });
                }
                pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                stack.push(ValueType::FuncRef);
            }
            0x26 => {
                let table_index = read_u32(code, &mut pc, function, offset)?;
                if table_index != 0 || table_index as usize >= module.table_count() {
                    return Err(ValidationError::TableIndexOutOfBounds {
                        function,
                        offset,
                        table_index,
                    });
                }
                pop_expect(&mut stack, &controls, ValueType::FuncRef, function, offset)?;
                pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
            }
            0x28..=0x35 => {
'''
replace_once(validator, old, new)

runtime = "crates/wasm-runtime/src/lib.rs"
old = '''                0x28..=0x35 => {
'''
new = '''                0x25 => {
                    let table_index = read_u32_immediate(code, &mut pc)?;
                    if table_index != 0 || self.table.is_none() {
                        return Err(RuntimeError::TableIndexOutOfBounds(table_index));
                    }
                    let element_index = numeric::i32_from_stack(&mut stack)? as u32;
                    let reference = self
                        .table
                        .as_ref()
                        .ok_or(RuntimeError::TableIndexOutOfBounds(table_index))?
                        .function_index_for_instance(element_index, &self.identity)
                        .map_err(|error| map_table_element_error(error, element_index))?;
                    stack.push(Value::FuncRef(reference));
                }
                0x26 => {
                    let table_index = read_u32_immediate(code, &mut pc)?;
                    if table_index != 0 || self.table.is_none() {
                        return Err(RuntimeError::TableIndexOutOfBounds(table_index));
                    }
                    let reference = match numeric::pop_typed(&mut stack, ValueType::FuncRef)? {
                        Value::FuncRef(reference) => reference,
                        _ => unreachable!("pop_typed established funcref"),
                    };
                    let element_index = numeric::i32_from_stack(&mut stack)? as u32;
                    let replacement = reference.map(|function_index| FunctionRef {
                        owner: Rc::downgrade(&self.identity),
                        function_index,
                    });
                    self.table
                        .as_ref()
                        .ok_or(RuntimeError::TableIndexOutOfBounds(table_index))?
                        .set(element_index, replacement)
                        .map_err(|error| map_table_element_error(error, element_index))?;
                }
                0x28..=0x35 => {
'''
replace_once(runtime, old, new)
replace_once(
    runtime,
    "0x0c | 0x0d | 0x10 | 0x20..=0x24 | 0x3f | 0x40 => {",
    "0x0c | 0x0d | 0x10 | 0x20..=0x26 | 0x3f | 0x40 => {",
)

Path("crates/wasm-runtime/tests/table_get_set.rs").write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, RuntimeError, TableHandle};
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

fn imported_table_module(body: &[u8]) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 0, 0]);

    let mut imports = vec![1];
    name(&mut imports, "env");
    name(&mut imports, "tab");
    imports.extend([1, 0x70, 0, 4]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[1, 0]);
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);
    section(&mut module, 9, &[1, 0, 0x41, 0, 0x0b, 2, 0, 0]);

    let mut code = vec![1, (body.len() + 1) as u8, 0];
    code.extend_from_slice(body);
    section(&mut module, 10, &code);
    module
}

fn owned_table_module(body: &[u8]) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 0, 0]);
    section(&mut module, 3, &[1, 0]);
    section(&mut module, 4, &[1, 0x70, 0, 4]);
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);
    section(&mut module, 9, &[1, 0, 0x41, 0, 0x0b, 2, 0, 0]);

    let mut code = vec![1, (body.len() + 1) as u8, 0];
    code.extend_from_slice(body);
    section(&mut module, 10, &code);
    module
}

fn hosts(table: &TableHandle) -> HostRegistry {
    let mut hosts = HostRegistry::new();
    hosts.register_table("env", "tab", table.clone()).unwrap();
    hosts
}

fn present(table: &TableHandle) -> Vec<bool> {
    (0..table.len())
        .map(|index| table.get(index).unwrap().is_some())
        .collect()
}

#[test]
fn round_trips_non_null_reference_in_imported_table() {
    let body = [0x41, 2, 0x41, 0, 0x25, 0, 0x26, 0, 0x0b];
    let table = TableHandle::new(4, Some(4)).unwrap();
    let mut instance = Instance::with_hosts(
        parse_module(&imported_table_module(&body)).unwrap(),
        hosts(&table),
    )
    .unwrap();

    instance.invoke_export("run", &[]).unwrap();
    assert_eq!(present(&table), vec![true, true, true, false]);
}

#[test]
fn table_set_clears_imported_slot_with_null() {
    let body = [0x41, 1, 0xd0, 0x70, 0x26, 0, 0x0b];
    let table = TableHandle::new(4, Some(4)).unwrap();
    let mut instance = Instance::with_hosts(
        parse_module(&imported_table_module(&body)).unwrap(),
        hosts(&table),
    )
    .unwrap();

    instance.invoke_export("run", &[]).unwrap();
    assert_eq!(present(&table), vec![true, false, false, false]);
}

#[test]
fn owned_table_get_set_executes_non_null_round_trip() {
    let body = [
        0x41, 2, 0x41, 0, 0x25, 0, 0x26, 0, 0x41, 2, 0x25, 0, 0xd1, 0x1a, 0x0b,
    ];
    let mut instance = Instance::new(parse_module(&owned_table_module(&body)).unwrap()).unwrap();
    instance.invoke_export("run", &[]).unwrap();
}

#[test]
fn table_get_oob_traps() {
    let body = [0x41, 4, 0x25, 0, 0x1a, 0x0b];
    let table = TableHandle::new(4, Some(4)).unwrap();
    let mut instance = Instance::with_hosts(
        parse_module(&imported_table_module(&body)).unwrap(),
        hosts(&table),
    )
    .unwrap();

    assert!(matches!(
        instance.invoke_export("run", &[]),
        Err(RuntimeError::TableElementOutOfBounds(4))
    ));
}

#[test]
fn table_set_oob_is_atomic() {
    let body = [0x41, 4, 0xd2, 0, 0x26, 0, 0x0b];
    let table = TableHandle::new(4, Some(4)).unwrap();
    let mut instance = Instance::with_hosts(
        parse_module(&imported_table_module(&body)).unwrap(),
        hosts(&table),
    )
    .unwrap();

    assert!(matches!(
        instance.invoke_export("run", &[]),
        Err(RuntimeError::TableElementOutOfBounds(4))
    ));
    assert_eq!(present(&table), vec![true, true, false, false]);
}

#[test]
fn validator_rejects_nonzero_table_index() {
    let body = [0x41, 0, 0x25, 1, 0x1a, 0x0b];
    let table = TableHandle::new(4, Some(4)).unwrap();
    assert!(matches!(
        Instance::with_hosts(
            parse_module(&imported_table_module(&body)).unwrap(),
            hosts(&table)
        ),
        Err(RuntimeError::Validation(
            ValidationError::TableIndexOutOfBounds { table_index: 1, .. }
        ))
    ));
}

#[test]
fn validator_rejects_numeric_table_set_value() {
    let body = [0x41, 0, 0x41, 0, 0x26, 0, 0x0b];
    let table = TableHandle::new(4, Some(4)).unwrap();
    assert!(matches!(
        Instance::with_hosts(
            parse_module(&imported_table_module(&body)).unwrap(),
            hosts(&table)
        ),
        Err(RuntimeError::Validation(ValidationError::TypeMismatch { .. }))
    ));
}
''')

Path("docs/table-get-set.md").write_text('''# Table reference access: `table.get` / `table.set`

This slice adds executable core `table.get` (`0x25`) and `table.set` (`0x26`) for the current single-`funcref` table surface. Typed validation requires an `i32` element index and preserves `funcref` values; runtime reads and writes nullable and non-null references without encoding them as integers.

Reads validate that a stored function reference belongs to the current instance before materializing `Value::FuncRef`, and writes re-materialize non-null references with the current instance identity. The same path applies to module-owned and imported/shared `TableHandle` values. Out-of-bounds element accesses trap before mutation, unsupported non-zero table indices fail closed, and numeric operands cannot be substituted for references.

The slice intentionally keeps the existing one-table, `funcref`-only boundary. `table.grow`, multi-table, `externref`, reference-typed host ABI, and cross-instance reference transfer remain separate capabilities.
''')
