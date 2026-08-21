from pathlib import Path


def replace_once(text: str, old: str, new: str) -> str:
    assert old in text, old[:160]
    return text.replace(old, new, 1)


path = Path("crates/wasm-runtime/src/lib.rs")
text = path.read_text()

text = replace_once(
    text,
    '''pub enum HostRegistryError {
    DuplicateFunction { module: String, name: String },
    UnsupportedSignature,
}
''',
    '''pub enum HostRegistryError {
    DuplicateFunction { module: String, name: String },
    DuplicateGlobal { module: String, name: String },
    UnsupportedSignature,
}
''',
)

text = replace_once(
    text,
    '''            Self::DuplicateFunction { module, name } => {
                write!(f, "host function {module}.{name} is already registered")
            }
            Self::UnsupportedSignature => write!(
''',
    '''            Self::DuplicateFunction { module, name } => {
                write!(f, "host function {module}.{name} is already registered")
            }
            Self::DuplicateGlobal { module, name } => {
                write!(f, "host immutable global {module}.{name} is already registered")
            }
            Self::UnsupportedSignature => write!(
''',
)

text = replace_once(
    text,
    '''pub struct HostRegistry {
    functions: HashMap<(String, String), HostFunction>,
}
''',
    '''pub struct HostRegistry {
    functions: HashMap<(String, String), HostFunction>,
    immutable_globals: HashMap<(String, String), Value>,
}
''',
)

text = replace_once(
    text,
    '''        f.debug_struct("HostRegistry")
            .field("function_count", &self.functions.len())
            .finish()
''',
    '''        f.debug_struct("HostRegistry")
            .field("function_count", &self.functions.len())
            .field("immutable_global_count", &self.immutable_globals.len())
            .finish()
''',
)

text = replace_once(
    text,
    '''        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeLimits {
''',
    '''        Ok(())
    }

    pub fn register_immutable_global(
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeLimits {
''',
)

text = replace_once(
    text,
    '''    UnresolvedImport {
        module: String,
        name: String,
    },
    UnsupportedObjectImport {
''',
    '''    UnresolvedImport {
        module: String,
        name: String,
    },
    UnresolvedGlobalImport {
        module: String,
        name: String,
    },
    HostGlobalTypeMismatch {
        module: String,
        name: String,
        expected: ValueType,
        actual: ValueType,
    },
    UnsupportedObjectImport {
''',
)

text = replace_once(
    text,
    '''            Self::UnresolvedImport { module, name } => {
                write!(f, "unresolved host function import {module}.{name}")
            }
            Self::UnsupportedObjectImport { module, name, kind } => write!(
''',
    '''            Self::UnresolvedImport { module, name } => {
                write!(f, "unresolved host function import {module}.{name}")
            }
            Self::UnresolvedGlobalImport { module, name } => {
                write!(f, "unresolved host immutable global import {module}.{name}")
            }
            Self::HostGlobalTypeMismatch {
                module,
                name,
                expected,
                actual,
            } => write!(
                f,
                "registered host global {module}.{name} has type {actual:?}, expected {expected:?}"
            ),
            Self::UnsupportedObjectImport { module, name, kind } => write!(
''',
)

text = replace_once(
    text,
    '''        let globals = module
            .globals
            .iter()
            .map(|global| value_from_constant(global.init))
            .collect();
''',
    '''        let globals = instantiate_globals(&module, &hosts)?;
''',
)

old_global_set = '''                0x24 => {
                    let index = read_u32_immediate(code, &mut pc)?;
                    let mutable = self
                        .module
                        .globals
                        .get(index as usize)
                        .ok_or(RuntimeError::GlobalOutOfBounds(index))?
                        .ty
                        .mutable;
                    if !mutable {
                        return Err(RuntimeError::ImmutableGlobalSet(index));
                    }
                    let expected = self.module.globals[index as usize].ty.value_type;
                    let value = numeric::pop_typed(&mut stack, expected)?;
                    *self
                        .globals
                        .get_mut(index as usize)
                        .ok_or(RuntimeError::GlobalOutOfBounds(index))? = value;
                }
'''
new_global_set = '''                0x24 => {
                    let index = read_u32_immediate(code, &mut pc)?;
                    let global_type = self
                        .module
                        .global_type(index)
                        .ok_or(RuntimeError::GlobalOutOfBounds(index))?;
                    if !global_type.mutable {
                        return Err(RuntimeError::ImmutableGlobalSet(index));
                    }
                    let value = numeric::pop_typed(&mut stack, global_type.value_type)?;
                    *self
                        .globals
                        .get_mut(index as usize)
                        .ok_or(RuntimeError::GlobalOutOfBounds(index))? = value;
                }
'''
text = replace_once(text, old_global_set, new_global_set)

old_reject = '''fn reject_unsupported_object_imports(module: &Module) -> Result<(), RuntimeError> {
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
'''
new_reject = '''fn reject_unsupported_object_imports(module: &Module) -> Result<(), RuntimeError> {
    for import in &module.imports {
        match import.desc {
            ImportDesc::Function(_) => {}
            ImportDesc::Global(global_type) if !global_type.mutable => {}
            _ => {
                return Err(RuntimeError::UnsupportedObjectImport {
                    module: import.module.clone(),
                    name: import.name.clone(),
                    kind: import.kind(),
                });
            }
        }
    }
    Ok(())
}
'''
text = replace_once(text, old_reject, new_reject)

old_bindings_tail = '''    }
    Ok(())
}

fn ensure_runtime_memory_index(instance: &Instance, index: u32) -> Result<(), RuntimeError> {
'''
new_bindings_tail = '''    }

    for import in &module.imports {
        let ImportDesc::Global(global_type) = import.desc else {
            continue;
        };
        if global_type.mutable {
            continue;
        }
        let key = (import.module.clone(), import.name.clone());
        let value = hosts
            .immutable_globals
            .get(&key)
            .copied()
            .ok_or_else(|| RuntimeError::UnresolvedGlobalImport {
                module: import.module.clone(),
                name: import.name.clone(),
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
    }
    Ok(())
}

fn instantiate_globals(module: &Module, hosts: &HostRegistry) -> Result<Vec<Value>, RuntimeError> {
    let mut globals = Vec::with_capacity(module.global_count());
    for import in &module.imports {
        let ImportDesc::Global(global_type) = import.desc else {
            continue;
        };
        if global_type.mutable {
            return Err(RuntimeError::UnsupportedObjectImport {
                module: import.module.clone(),
                name: import.name.clone(),
                kind: ImportKind::Global,
            });
        }
        let key = (import.module.clone(), import.name.clone());
        let value = hosts
            .immutable_globals
            .get(&key)
            .copied()
            .ok_or_else(|| RuntimeError::UnresolvedGlobalImport {
                module: import.module.clone(),
                name: import.name.clone(),
            })?;
        numeric::expect_type(value, global_type.value_type)?;
        globals.push(value);
    }
    globals.extend(
        module
            .globals
            .iter()
            .map(|global| value_from_constant(global.init)),
    );
    Ok(globals)
}

fn ensure_runtime_memory_index(instance: &Instance, index: u32) -> Result<(), RuntimeError> {
'''
text = replace_once(text, old_bindings_tail, new_bindings_tail)
path.write_text(text)

# Existing object-import test: immutable globals are now a supported binding kind,
# so absence of the binding is the precise runtime failure.
path = Path("crates/wasm-runtime/tests/phase5c_imports.rs")
text = path.read_text()
old = '''    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::UnsupportedObjectImport {
            kind: ImportKind::Global,
            ..
        })
    ));
'''
new = '''    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::UnresolvedGlobalImport { .. })
    ));
'''
text = replace_once(text, old, new)
path.write_text(text)

# New vertical-slice integration tests.
path = Path("crates/wasm-runtime/tests/phase5c_imported_globals.rs")
assert not path.exists()
path.write_text(r'''use wasm_parser::{parse_module, ImportKind};
use wasm_runtime::{HostRegistry, HostRegistryError, Instance, RuntimeError, Value};

const I32: u8 = 0x7f;
const I64: u8 = 0x7e;
const F32: u8 = 0x7d;
const F64: u8 = 0x7c;

fn push_u32(bytes: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        bytes.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn push_name(bytes: &mut Vec<u8>, name: &str) {
    push_u32(bytes, name.len() as u32);
    bytes.extend_from_slice(name.as_bytes());
}

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn module_header() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

fn imported_global_getter(value_type: u8, mutable: bool) -> Vec<u8> {
    let mut module = module_header();
    push_section(
        &mut module,
        1,
        &[0x01, 0x60, 0x00, 0x01, value_type],
    );

    let mut imports = vec![0x01];
    push_name(&mut imports, "env");
    push_name(&mut imports, "g");
    imports.extend([0x03, value_type, u8::from(mutable)]);
    push_section(&mut module, 2, &imports);

    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(
        &mut module,
        7,
        &[
            0x02,
            0x03, b'r', b'u', b'n', 0x00, 0x00,
            0x01, b'g', 0x03, 0x00,
        ],
    );
    push_section(&mut module, 10, &[0x01, 0x04, 0x00, 0x23, 0x00, 0x0b]);
    module
}

fn imported_and_defined_global_module() -> Vec<u8> {
    let mut module = module_header();
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x01, I32]);

    let mut imports = vec![0x01];
    push_name(&mut imports, "env");
    push_name(&mut imports, "base");
    imports.extend([0x03, I32, 0x00]);
    push_section(&mut module, 2, &imports);

    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 6, &[0x01, I32, 0x01, 0x41, 0x00, 0x0b]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);

    let body = [
        0x00, // no locals
        0x41, 0x09, // i32.const 9
        0x24, 0x01, // global.set 1 (defined mutable global)
        0x23, 0x00, // global.get 0 (imported immutable global)
        0x23, 0x01, // global.get 1
        0x6a, // i32.add
        0x0b,
    ];
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend(body);
    push_section(&mut module, 10, &code);
    module
}

fn instantiate_with_global(bytes: &[u8], name: &str, value: Value) -> Instance {
    let module = parse_module(bytes).expect("parse imported-global fixture");
    let mut hosts = HostRegistry::new();
    hosts
        .register_immutable_global("env", name, value)
        .expect("register immutable global");
    Instance::with_hosts(module, hosts).expect("instantiate imported-global fixture")
}

#[test]
fn immutable_imported_globals_execute_for_all_numeric_types() {
    let cases = [
        (I32, Value::I32(-7)),
        (I64, Value::I64(0x1_0000_0001)),
        (F32, Value::F32(1.25)),
        (F64, Value::F64(-9.5)),
    ];

    for (value_type, value) in cases {
        let module = imported_global_getter(value_type, false);
        let mut vm = instantiate_with_global(&module, "g", value);
        assert_eq!(vm.global(0), Some(value));
        assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(value));
    }
}

#[test]
fn imported_global_precedes_defined_global_in_runtime_index_space() {
    let module = imported_and_defined_global_module();
    let mut vm = instantiate_with_global(&module, "base", Value::I32(33));
    assert_eq!(vm.global(0), Some(Value::I32(33)));
    assert_eq!(vm.global(1), Some(Value::I32(0)));
    assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(42)));
    assert_eq!(vm.global(1), Some(Value::I32(9)));
}

#[test]
fn missing_immutable_global_binding_is_distinct_from_unsupported_object_import() {
    let module = parse_module(&imported_global_getter(I32, false)).unwrap();
    assert!(matches!(
        Instance::new(module),
        Err(RuntimeError::UnresolvedGlobalImport { ref module, ref name })
            if module == "env" && name == "g"
    ));
}

#[test]
fn immutable_global_binding_requires_exact_numeric_type() {
    let module = parse_module(&imported_global_getter(I32, false)).unwrap();
    let mut hosts = HostRegistry::new();
    hosts
        .register_immutable_global("env", "g", Value::I64(7))
        .unwrap();
    assert!(matches!(
        Instance::with_hosts(module, hosts),
        Err(RuntimeError::HostGlobalTypeMismatch {
            expected: wasm_parser::ValueType::I32,
            actual: wasm_parser::ValueType::I64,
            ..
        })
    ));
}

#[test]
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

#[test]
fn duplicate_immutable_global_registration_is_rejected() {
    let mut hosts = HostRegistry::new();
    hosts
        .register_immutable_global("env", "g", Value::I32(1))
        .unwrap();
    assert_eq!(
        hosts.register_immutable_global("env", "g", Value::I32(2)),
        Err(HostRegistryError::DuplicateGlobal {
            module: "env".into(),
            name: "g".into(),
        })
    );
}
''')
