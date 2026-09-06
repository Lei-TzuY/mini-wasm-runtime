from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    if text.count(old) != 1:
        raise SystemExit(f"expected exactly one anchor in {path}, found {text.count(old)}")
    p.write_text(text.replace(old, new, 1))


runtime = "crates/wasm-runtime/src/lib.rs"
replace_once(
    runtime,
    '''    fn bind(&self, owner: &Rc<()>) -> Result<(), TableHandleError> {
''',
    '''    fn grow(&self, delta: u32, fill: Option<FunctionRef>) -> i32 {
        let previous = self.len();
        let Some(new_length) = previous.checked_add(delta) else {
            return -1;
        };
        if self.maximum.is_some_and(|maximum| new_length > maximum) {
            return -1;
        }
        if delta == 0 {
            return previous as i32;
        }
        let additional = delta as usize;
        let new_length = new_length as usize;
        let mut slots = self.slots.borrow_mut();
        if slots.try_reserve_exact(additional).is_err() {
            return -1;
        }
        slots.resize(new_length, fill);
        previous as i32
    }

    fn bind(&self, owner: &Rc<()>) -> Result<(), TableHandleError> {
''',
)
replace_once(
    runtime,
    '''                        16 => {
                            let table_index = read_u32_immediate(code, &mut pc)?;
                            stack.push(Value::I32(self.table_size(table_index)?));
                        }
''',
    '''                        15 => {
                            let table_index = read_u32_immediate(code, &mut pc)?;
                            if table_index != 0 || self.table.is_none() {
                                return Err(RuntimeError::TableIndexOutOfBounds(table_index));
                            }
                            let delta = numeric::i32_from_stack(&mut stack)? as u32;
                            let reference = match numeric::pop_typed(&mut stack, ValueType::FuncRef)? {
                                Value::FuncRef(reference) => reference,
                                _ => unreachable!("pop_typed established funcref"),
                            };
                            let fill = reference.map(|function_index| FunctionRef {
                                owner: Rc::downgrade(&self.identity),
                                function_index,
                            });
                            let previous = self
                                .table
                                .as_ref()
                                .ok_or(RuntimeError::TableIndexOutOfBounds(table_index))?
                                .grow(delta, fill);
                            stack.push(Value::I32(previous));
                        }
                        16 => {
                            let table_index = read_u32_immediate(code, &mut pc)?;
                            stack.push(Value::I32(self.table_size(table_index)?));
                        }
''',
)
replace_once(
    runtime,
    '''                    16 | 17 => {
                        let _ = read_u32_immediate(code, &mut pc)?;
                    }
''',
    '''                    15 | 16 | 17 => {
                        let _ = read_u32_immediate(code, &mut pc)?;
                    }
''',
)

validator = "crates/wasm-validator/src/typed.rs"
replace_once(
    validator,
    '''                    16 => {
                        let table_index = read_u32(code, &mut pc, function, offset)?;
                        if table_index != 0 || table_index as usize >= module.table_count() {
                            return Err(ValidationError::TableIndexOutOfBounds {
                                function,
                                offset,
                                table_index,
                            });
                        }
                        stack.push(ValueType::I32);
                    }
''',
    '''                    15 => {
                        let table_index = read_u32(code, &mut pc, function, offset)?;
                        if table_index != 0 || table_index as usize >= module.table_count() {
                            return Err(ValidationError::TableIndexOutOfBounds {
                                function,
                                offset,
                                table_index,
                            });
                        }
                        pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::FuncRef, function, offset)?;
                        stack.push(ValueType::I32);
                    }
                    16 => {
                        let table_index = read_u32(code, &mut pc, function, offset)?;
                        if table_index != 0 || table_index as usize >= module.table_count() {
                            return Err(ValidationError::TableIndexOutOfBounds {
                                function,
                                offset,
                                table_index,
                            });
                        }
                        stack.push(ValueType::I32);
                    }
''',
)

Path("crates/wasm-runtime/tests/bulk_table_grow.rs").write_text(r'''use wasm_parser::parse_module;
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

fn module(body: &[u8], table_index: u32, minimum: u8, maximum: u8) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 0, 1, 0x7f]);

    let mut imports = vec![1];
    name(&mut imports, "env");
    name(&mut imports, "tab");
    imports.extend([1, 0x70, 1, minimum, maximum]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[1, 0]);
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);

    let mut function_body = vec![0];
    function_body.extend_from_slice(body);
    function_body.extend([0xfc, 15]);
    u32leb(&mut function_body, table_index);
    function_body.push(0x0b);
    let mut code = vec![1];
    u32leb(&mut code, function_body.len() as u32);
    code.extend_from_slice(&function_body);
    section(&mut module, 10, &code);
    module
}

fn hosts(table: &TableHandle) -> HostRegistry {
    let mut hosts = HostRegistry::new();
    hosts.register_table("env", "tab", table.clone()).unwrap();
    hosts
}

#[test]
fn grows_imported_table_with_null_and_returns_previous_size() {
    let table = TableHandle::new(2, Some(4)).unwrap();
    let body = [0xd0, 0x70, 0x41, 0x02];
    let mut vm = Instance::with_hosts(parse_module(&module(&body, 0, 2, 4)).unwrap(), hosts(&table)).unwrap();

    assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(2)));
    assert_eq!(table.len(), 4);
    assert!(table.get(2).unwrap().is_none());
    assert!(table.get(3).unwrap().is_none());
}

#[test]
fn grows_with_non_null_funcref_initializer() {
    let table = TableHandle::new(1, Some(3)).unwrap();
    let body = [0xd2, 0x00, 0x41, 0x01];
    let mut vm = Instance::with_hosts(parse_module(&module(&body, 0, 1, 3)).unwrap(), hosts(&table)).unwrap();

    assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(1)));
    assert_eq!(table.len(), 2);
    assert!(table.get(1).unwrap().is_some());
}

#[test]
fn zero_delta_returns_size_without_mutation() {
    let table = TableHandle::new(2, Some(2)).unwrap();
    let body = [0xd0, 0x70, 0x41, 0x00];
    let mut vm = Instance::with_hosts(parse_module(&module(&body, 0, 2, 2)).unwrap(), hosts(&table)).unwrap();

    assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(2)));
    assert_eq!(table.len(), 2);
}

#[test]
fn maximum_failure_returns_minus_one_and_is_atomic() {
    let table = TableHandle::new(2, Some(2)).unwrap();
    let body = [0xd2, 0x00, 0x41, 0x01];
    let mut vm = Instance::with_hosts(parse_module(&module(&body, 0, 2, 2)).unwrap(), hosts(&table)).unwrap();

    assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(-1)));
    assert_eq!(table.len(), 2);
    assert!(table.get(0).unwrap().is_none());
    assert!(table.get(1).unwrap().is_none());
}

#[test]
fn validator_rejects_nonzero_table_index() {
    let table = TableHandle::new(1, Some(2)).unwrap();
    let body = [0xd0, 0x70, 0x41, 0x01];
    assert!(matches!(
        Instance::with_hosts(parse_module(&module(&body, 1, 1, 2)).unwrap(), hosts(&table)),
        Err(wasm_runtime::RuntimeError::Validation(
            ValidationError::TableIndexOutOfBounds { table_index: 1, .. }
        ))
    ));
}

#[test]
fn validator_rejects_numeric_initializer() {
    let table = TableHandle::new(1, Some(2)).unwrap();
    let body = [0x41, 0x00, 0x41, 0x01];
    assert!(matches!(
        Instance::with_hosts(parse_module(&module(&body, 0, 1, 2)).unwrap(), hosts(&table)),
        Err(wasm_runtime::RuntimeError::Validation(ValidationError::TypeMismatch { .. }))
    ));
}
''')

Path("docs/bulk-table-grow.md").write_text('''# Bulk table: `table.grow`

This slice adds executable `table.grow` (`0xfc 15`) for the current single-`funcref` table surface. Typed validation consumes a `funcref` initializer and an `i32` delta and returns the previous table size as `i32`.

Runtime growth applies to both module-owned and imported/shared `TableHandle` values. New slots are initialized with either null or an instance-owned non-null function reference. A zero delta returns the current size without mutation. Growth beyond the declared maximum, integer overflow, or allocation failure returns `-1` and leaves the table unchanged.

The slice intentionally preserves the existing one-table, `funcref`-only boundary. Multi-table, `externref`, memory64, and reference-typed host ABI remain separate capabilities.
''')
