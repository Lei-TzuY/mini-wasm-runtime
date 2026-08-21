from pathlib import Path


def replace_once(text: str, old: str, new: str) -> str:
    assert old in text, old[:200]
    return text.replace(old, new, 1)


path = Path("crates/wasm-runtime/src/lib.rs")
text = path.read_text()

text = replace_once(
    text,
    'use std::{collections::HashMap, fmt};',
    'use std::{cell::RefCell, collections::HashMap, fmt, rc::Rc};',
)

insert_after = '''impl std::error::Error for HostError {}\n\n'''
assert insert_after in text
shared = r'''#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalHandleError {
    Immutable,
    TypeMismatch {
        expected: ValueType,
        actual: ValueType,
    },
}

impl fmt::Display for GlobalHandleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Immutable => write!(f, "global is immutable"),
            Self::TypeMismatch { expected, actual } => {
                write!(f, "global expects {expected:?}, got {actual:?}")
            }
        }
    }
}

impl std::error::Error for GlobalHandleError {}

#[derive(Debug, Clone)]
pub struct GlobalHandle {
    value: Rc<RefCell<Value>>,
    value_type: ValueType,
    mutable: bool,
}

impl GlobalHandle {
    pub fn immutable(value: Value) -> Self {
        Self::new(value, false)
    }

    pub fn mutable(value: Value) -> Self {
        Self::new(value, true)
    }

    fn new(value: Value, mutable: bool) -> Self {
        Self {
            value: Rc::new(RefCell::new(value)),
            value_type: value.value_type(),
            mutable,
        }
    }

    pub fn value_type(&self) -> ValueType {
        self.value_type
    }

    pub fn is_mutable(&self) -> bool {
        self.mutable
    }

    pub fn get(&self) -> Value {
        *self.value.borrow()
    }

    pub fn set(&self, value: Value) -> Result<(), GlobalHandleError> {
        if !self.mutable {
            return Err(GlobalHandleError::Immutable);
        }
        let actual = value.value_type();
        if actual != self.value_type {
            return Err(GlobalHandleError::TypeMismatch {
                expected: self.value_type,
                actual,
            });
        }
        *self.value.borrow_mut() = value;
        Ok(())
    }
}

'''
text = text.replace(insert_after, insert_after + shared, 1)

text = replace_once(
    text,
    '''pub struct HostRegistry {
    functions: HashMap<(String, String), HostFunction>,
    immutable_globals: HashMap<(String, String), Value>,
}
''',
    '''pub struct HostRegistry {
    functions: HashMap<(String, String), HostFunction>,
    globals: HashMap<(String, String), GlobalHandle>,
}
''',
)
text = replace_once(
    text,
    '.field("immutable_global_count", &self.immutable_globals.len())',
    '.field("global_count", &self.globals.len())',
)

old_register = '''    pub fn register_immutable_global(
        &mut self,
        module: impl Into<String>,
        name: impl Into<String>,
        value: Value,
    ) -> Result<(), HostRegistryError> {
        let module = module.into();
        let name = name.into();
        let key = (module.clone(), name.clone());
        if self.immutable_globals.contains_key(&key) {
            return Err(HostRegistryError::DuplicateGlobal { module, name });
        }
        self.immutable_globals.insert(key, value);
        Ok(())
    }
'''
new_register = '''    pub fn register_immutable_global(
        &mut self,
        module: impl Into<String>,
        name: impl Into<String>,
        value: Value,
    ) -> Result<(), HostRegistryError> {
        self.register_global(module, name, GlobalHandle::immutable(value))
    }

    pub fn register_global(
        &mut self,
        module: impl Into<String>,
        name: impl Into<String>,
        global: GlobalHandle,
    ) -> Result<(), HostRegistryError> {
        let module = module.into();
        let name = name.into();
        let key = (module.clone(), name.clone());
        if self.globals.contains_key(&key) {
            return Err(HostRegistryError::DuplicateGlobal { module, name });
        }
        self.globals.insert(key, global);
        Ok(())
    }
'''
text = replace_once(text, old_register, new_register)

text = replace_once(
    text,
    '''    HostGlobalTypeMismatch {
        module: String,
        name: String,
        expected: ValueType,
        actual: ValueType,
    },
''',
    '''    HostGlobalTypeMismatch {
        module: String,
        name: String,
        expected: ValueType,
        actual: ValueType,
    },
    HostGlobalMutabilityMismatch {
        module: String,
        name: String,
        expected: bool,
        actual: bool,
    },
''',
)
text = replace_once(
    text,
    '''            Self::HostGlobalTypeMismatch {
                module,
                name,
                expected,
                actual,
            } => write!(
                f,
                "registered host global {module}.{name} has type {actual:?}, expected {expected:?}"
            ),
''',
    '''            Self::HostGlobalTypeMismatch {
                module,
                name,
                expected,
                actual,
            } => write!(
                f,
                "registered host global {module}.{name} has type {actual:?}, expected {expected:?}"
            ),
            Self::HostGlobalMutabilityMismatch {
                module,
                name,
                expected,
                actual,
            } => write!(
                f,
                "registered host global {module}.{name} has mutable={actual}, expected mutable={expected}"
            ),
''',
)

text = replace_once(
    text,
    '    globals: Vec<Value>,',
    '    globals: Vec<GlobalHandle>,',
)
text = replace_once(
    text,
    '''    pub fn global(&self, index: u32) -> Option<Value> {
        self.globals.get(index as usize).copied()
    }
''',
    '''    pub fn global(&self, index: u32) -> Option<Value> {
        self.globals.get(index as usize).map(GlobalHandle::get)
    }
''',
)
text = replace_once(
    text,
    '''                0x23 => {
                    let index = read_u32_immediate(code, &mut pc)?;
                    let value = *self
                        .globals
                        .get(index as usize)
                        .ok_or(RuntimeError::GlobalOutOfBounds(index))?;
                    stack.push(value);
                }
''',
    '''                0x23 => {
                    let index = read_u32_immediate(code, &mut pc)?;
                    let value = self
                        .globals
                        .get(index as usize)
                        .ok_or(RuntimeError::GlobalOutOfBounds(index))?
                        .get();
                    stack.push(value);
                }
''',
)
text = replace_once(
    text,
    '''                    *self
                        .globals
                        .get_mut(index as usize)
                        .ok_or(RuntimeError::GlobalOutOfBounds(index))? = value;
''',
    '''                    self.globals
                        .get(index as usize)
                        .ok_or(RuntimeError::GlobalOutOfBounds(index))?
                        .set(value)
                        .map_err(|error| match error {
                            GlobalHandleError::Immutable => RuntimeError::ImmutableGlobalSet(index),
                            GlobalHandleError::TypeMismatch { expected, actual } => {
                                RuntimeError::ValueTypeMismatch { expected, actual }
                            }
                        })?;
''',
)

old_reject = '''        match import.desc {
            ImportDesc::Function(_) => {}
            ImportDesc::Global(global_type) if !global_type.mutable => {}
            _ => {
'''
new_reject = '''        match import.desc {
            ImportDesc::Function(_) | ImportDesc::Global(_) => {}
            _ => {
'''
text = replace_once(text, old_reject, new_reject)

old_validation = '''        let ImportDesc::Global(global_type) = import.desc else {
            continue;
        };
        if global_type.mutable {
            continue;
        }
        let key = (import.module.clone(), import.name.clone());
        let value = hosts.immutable_globals.get(&key).copied().ok_or_else(|| {
            RuntimeError::UnresolvedGlobalImport {
                module: import.module.clone(),
                name: import.name.clone(),
            }
        })?;
        let actual = value.value_type();
        if actual != global_type.value_type {
            return Err(RuntimeError::HostGlobalTypeMismatch {
                module: import.module.clone(),
                name: import.name.clone(),
                expected: global_type.value_type,
                actual,
            });
        }
'''
new_validation = '''        let ImportDesc::Global(global_type) = import.desc else {
            continue;
        };
        let key = (import.module.clone(), import.name.clone());
        let global = hosts.globals.get(&key).ok_or_else(|| {
            RuntimeError::UnresolvedGlobalImport {
                module: import.module.clone(),
                name: import.name.clone(),
            }
        })?;
        let actual = global.value_type();
        if actual != global_type.value_type {
            return Err(RuntimeError::HostGlobalTypeMismatch {
                module: import.module.clone(),
                name: import.name.clone(),
                expected: global_type.value_type,
                actual,
            });
        }
        if global.is_mutable() != global_type.mutable {
            return Err(RuntimeError::HostGlobalMutabilityMismatch {
                module: import.module.clone(),
                name: import.name.clone(),
                expected: global_type.mutable,
                actual: global.is_mutable(),
            });
        }
'''
text = replace_once(text, old_validation, new_validation)

start = text.index('fn instantiate_globals(')
end = text.index('\nfn ensure_runtime_memory_index', start)
new_inst = r'''fn instantiate_globals(
    module: &Module,
    hosts: &HostRegistry,
) -> Result<Vec<GlobalHandle>, RuntimeError> {
    let mut globals = Vec::with_capacity(module.global_count());
    for import in &module.imports {
        let ImportDesc::Global(global_type) = import.desc else {
            continue;
        };
        let key = (import.module.clone(), import.name.clone());
        let global = hosts.globals.get(&key).cloned().ok_or_else(|| {
            RuntimeError::UnresolvedGlobalImport {
                module: import.module.clone(),
                name: import.name.clone(),
            }
        })?;
        if global.value_type() != global_type.value_type {
            return Err(RuntimeError::HostGlobalTypeMismatch {
                module: import.module.clone(),
                name: import.name.clone(),
                expected: global_type.value_type,
                actual: global.value_type(),
            });
        }
        if global.is_mutable() != global_type.mutable {
            return Err(RuntimeError::HostGlobalMutabilityMismatch {
                module: import.module.clone(),
                name: import.name.clone(),
                expected: global_type.mutable,
                actual: global.is_mutable(),
            });
        }
        globals.push(global);
    }
    globals.extend(module.globals.iter().map(|global| {
        GlobalHandle::new(value_from_constant(global.init), global.ty.mutable)
    }));
    Ok(globals)
}
'''
text = text[:start] + new_inst + text[end:]
path.write_text(text)

# Replace imported-global tests with shared-state coverage while preserving immutable coverage.
path = Path("crates/wasm-runtime/tests/phase5c_imported_globals.rs")
text = path.read_text()
text = text.replace(
    'use wasm_parser::{parse_module, ImportKind};\nuse wasm_runtime::{HostRegistry, HostRegistryError, Instance, RuntimeError, Value};',
    'use wasm_parser::parse_module;\nuse wasm_runtime::{GlobalHandle, GlobalHandleError, HostRegistry, HostRegistryError, Instance, RuntimeError, Value};',
    1,
)
# Mutable fail-closed test becomes true aliasing test; add helper module with getter/setter.
old_test = r'''#[test]
fn mutable_global_import_remains_fail_closed() {
    let module = parse_module(&imported_global_getter(I32, true)).unwrap();
    let mut hosts = HostRegistry::new();
    hosts
        .register_immutable_global("env", "g", Value::I32(7))
        .unwrap();
    assert!(matches!(
        Instance::with_hosts(module, hosts),
        Err(RuntimeError::UnsupportedObjectImport {
            kind: ImportKind::Global,
            ..
        })
    ));
}
'''
new_test = r'''fn mutable_global_accessors() -> Vec<u8> {
    let mut module = module_header();
    let types = [
        0x02,
        0x60, 0x00, 0x01, I32, // [] -> i32
        0x60, 0x01, I32, 0x00, // [i32] -> []
    ];
    push_section(&mut module, 1, &types);

    let mut imports = vec![0x01];
    push_name(&mut imports, "env");
    push_name(&mut imports, "g");
    imports.extend([0x03, I32, 0x01]);
    push_section(&mut module, 2, &imports);

    push_section(&mut module, 3, &[0x02, 0x00, 0x01]);
    push_section(
        &mut module,
        7,
        &[
            0x02,
            0x03, b'g', b'e', b't', 0x00, 0x00,
            0x03, b's', b'e', b't', 0x00, 0x01,
        ],
    );

    let getter = [0x00, 0x23, 0x00, 0x0b];
    let setter = [0x00, 0x20, 0x00, 0x24, 0x00, 0x0b];
    let mut code = vec![0x02];
    push_u32(&mut code, getter.len() as u32);
    code.extend(getter);
    push_u32(&mut code, setter.len() as u32);
    code.extend(setter);
    push_section(&mut module, 10, &code);
    module
}

#[test]
fn mutable_global_import_preserves_bidirectional_aliasing() {
    let module = parse_module(&mutable_global_accessors()).unwrap();
    let global = GlobalHandle::mutable(Value::I32(7));
    let mut hosts = HostRegistry::new();
    hosts.register_global("env", "g", global.clone()).unwrap();
    let mut vm = Instance::with_hosts(module, hosts).unwrap();

    global.set(Value::I32(11)).unwrap();
    assert_eq!(vm.invoke_export("get", &[]).unwrap(), Some(Value::I32(11)));

    assert_eq!(vm.invoke_export("set", &[Value::I32(42)]).unwrap(), None);
    assert_eq!(global.get(), Value::I32(42));
    assert_eq!(vm.global(0), Some(Value::I32(42)));
}

#[test]
fn imported_global_mutability_must_match_exactly() {
    let mutable_module = parse_module(&imported_global_getter(I32, true)).unwrap();
    let mut immutable_hosts = HostRegistry::new();
    immutable_hosts
        .register_immutable_global("env", "g", Value::I32(7))
        .unwrap();
    assert!(matches!(
        Instance::with_hosts(mutable_module, immutable_hosts),
        Err(RuntimeError::HostGlobalMutabilityMismatch {
            expected: true,
            actual: false,
            ..
        })
    ));

    let immutable_module = parse_module(&imported_global_getter(I32, false)).unwrap();
    let mut mutable_hosts = HostRegistry::new();
    mutable_hosts
        .register_global("env", "g", GlobalHandle::mutable(Value::I32(7)))
        .unwrap();
    assert!(matches!(
        Instance::with_hosts(immutable_module, mutable_hosts),
        Err(RuntimeError::HostGlobalMutabilityMismatch {
            expected: false,
            actual: true,
            ..
        })
    ));
}

#[test]
fn global_handle_enforces_mutability_and_type_outside_the_vm() {
    let immutable = GlobalHandle::immutable(Value::I32(1));
    assert_eq!(immutable.set(Value::I32(2)), Err(GlobalHandleError::Immutable));

    let mutable = GlobalHandle::mutable(Value::I32(1));
    assert_eq!(
        mutable.set(Value::I64(2)),
        Err(GlobalHandleError::TypeMismatch {
            expected: wasm_parser::ValueType::I32,
            actual: wasm_parser::ValueType::I64,
        })
    );
    assert_eq!(mutable.get(), Value::I32(1));
}
'''
assert old_test in text
text = text.replace(old_test, new_test, 1)
path.write_text(text)
