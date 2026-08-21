from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


runtime_path = Path("crates/wasm-runtime/src/lib.rs")
runtime = runtime_path.read_text()

runtime = replace_once(
    runtime,
    "use std::{cell::RefCell, collections::HashMap, fmt, rc::Rc};",
    "use std::{\n    cell::RefCell,\n    collections::HashMap,\n    fmt,\n    rc::{Rc, Weak},\n};",
    "std imports",
)

marker = "pub struct HostContext<'a> {\n"
insert = r'''#[derive(Clone)]
pub struct FunctionRef {
    owner: Weak<()>,
    function_index: u32,
}

impl fmt::Debug for FunctionRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FunctionRef(..)")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableHandleError {
    InvalidLimits { minimum: u32, maximum: u32 },
    AllocationFailed { elements: u32 },
    OutOfBounds { index: u32, length: u32 },
    ForeignFunctionReference { index: u32 },
    AlreadyBound,
}

impl fmt::Display for TableHandleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits { minimum, maximum } => write!(
                f,
                "table minimum {minimum} exceeds declared maximum {maximum}"
            ),
            Self::AllocationFailed { elements } => {
                write!(f, "failed to allocate table with {elements} elements")
            }
            Self::OutOfBounds { index, length } => {
                write!(f, "table element index {index} is out of bounds for length {length}")
            }
            Self::ForeignFunctionReference { index } => write!(
                f,
                "table element {index} contains a function reference from another instance"
            ),
            Self::AlreadyBound => write!(f, "table is already bound to a live instance"),
        }
    }
}

impl std::error::Error for TableHandleError {}

#[derive(Clone)]
pub struct TableHandle {
    slots: Rc<RefCell<Vec<Option<FunctionRef>>>>,
    maximum: Option<u32>,
    owner: Rc<RefCell<Option<Weak<()>>>>,
}

impl fmt::Debug for TableHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TableHandle")
            .field("length", &self.len())
            .field("maximum", &self.maximum)
            .finish()
    }
}

impl TableHandle {
    pub fn new(minimum: u32, maximum: Option<u32>) -> Result<Self, TableHandleError> {
        if let Some(maximum) = maximum {
            if minimum > maximum {
                return Err(TableHandleError::InvalidLimits { minimum, maximum });
            }
        }
        let length = usize::try_from(minimum)
            .map_err(|_| TableHandleError::AllocationFailed { elements: minimum })?;
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(length)
            .map_err(|_| TableHandleError::AllocationFailed { elements: minimum })?;
        slots.resize(length, None);
        Ok(Self {
            slots: Rc::new(RefCell::new(slots)),
            maximum,
            owner: Rc::new(RefCell::new(None)),
        })
    }

    pub fn len(&self) -> u32 {
        u32::try_from(self.slots.borrow().len())
            .expect("table length originates from a u32 minimum")
    }

    pub fn is_empty(&self) -> bool {
        self.slots.borrow().is_empty()
    }

    pub fn maximum(&self) -> Option<u32> {
        self.maximum
    }

    pub fn get(&self, index: u32) -> Result<Option<FunctionRef>, TableHandleError> {
        self.slots
            .borrow()
            .get(index as usize)
            .cloned()
            .ok_or_else(|| TableHandleError::OutOfBounds {
                index,
                length: self.len(),
            })
    }

    pub fn set(
        &self,
        index: u32,
        function: Option<FunctionRef>,
    ) -> Result<(), TableHandleError> {
        let length = self.len();
        let mut slots = self.slots.borrow_mut();
        let slot = slots
            .get_mut(index as usize)
            .ok_or(TableHandleError::OutOfBounds { index, length })?;
        *slot = function;
        Ok(())
    }

    fn bind(&self, owner: &Rc<()>) -> Result<(), TableHandleError> {
        let mut binding = self.owner.borrow_mut();
        if let Some(existing) = binding.as_ref().and_then(Weak::upgrade) {
            if !Rc::ptr_eq(&existing, owner) {
                return Err(TableHandleError::AlreadyBound);
            }
            return Ok(());
        }
        *binding = Some(Rc::downgrade(owner));
        Ok(())
    }

    fn set_for_instance(
        &self,
        index: u32,
        function_index: u32,
        owner: &Rc<()>,
    ) -> Result<(), TableHandleError> {
        self.bind(owner)?;
        self.set(
            index,
            Some(FunctionRef {
                owner: Rc::downgrade(owner),
                function_index,
            }),
        )
    }

    fn function_index_for_instance(
        &self,
        index: u32,
        owner: &Rc<()>,
    ) -> Result<Option<u32>, TableHandleError> {
        let Some(function) = self.get(index)? else {
            return Ok(None);
        };
        let Some(actual_owner) = function.owner.upgrade() else {
            return Err(TableHandleError::ForeignFunctionReference { index });
        };
        if !Rc::ptr_eq(&actual_owner, owner) {
            return Err(TableHandleError::ForeignFunctionReference { index });
        }
        Ok(Some(function.function_index))
    }
}

'''
runtime = replace_once(runtime, marker, insert + marker, "table handle insertion")

runtime = replace_once(
    runtime,
    "    DuplicateGlobal { module: String, name: String },\n    UnsupportedSignature,",
    "    DuplicateGlobal { module: String, name: String },\n    DuplicateTable { module: String, name: String },\n    UnsupportedSignature,",
    "registry error variant",
)

runtime = replace_once(
    runtime,
    '''            Self::DuplicateGlobal { module, name } => {
                write!(
                    f,
                    "host immutable global {module}.{name} is already registered"
                )
            }
            Self::UnsupportedSignature => write!(''',
    '''            Self::DuplicateGlobal { module, name } => {
                write!(f, "host global {module}.{name} is already registered")
            }
            Self::DuplicateTable { module, name } => {
                write!(f, "host table {module}.{name} is already registered")
            }
            Self::UnsupportedSignature => write!(''',
    "registry error display",
)

runtime = replace_once(
    runtime,
    '''pub struct HostRegistry {
    functions: HashMap<(String, String), HostFunction>,
    globals: HashMap<(String, String), GlobalHandle>,
}''',
    '''pub struct HostRegistry {
    functions: HashMap<(String, String), HostFunction>,
    globals: HashMap<(String, String), GlobalHandle>,
    tables: HashMap<(String, String), TableHandle>,
}''',
    "registry fields",
)

runtime = replace_once(
    runtime,
    '''            .field("function_count", &self.functions.len())
            .field("global_count", &self.globals.len())
            .finish()''',
    '''            .field("function_count", &self.functions.len())
            .field("global_count", &self.globals.len())
            .field("table_count", &self.tables.len())
            .finish()''',
    "registry debug",
)

runtime = replace_once(
    runtime,
    '''        self.globals.insert(key, global);
        Ok(())
    }
}''',
    '''        self.globals.insert(key, global);
        Ok(())
    }

    pub fn register_table(
        &mut self,
        module: impl Into<String>,
        name: impl Into<String>,
        table: TableHandle,
    ) -> Result<(), HostRegistryError> {
        let module = module.into();
        let name = name.into();
        let key = (module.clone(), name.clone());
        if self.tables.contains_key(&key) {
            return Err(HostRegistryError::DuplicateTable { module, name });
        }
        self.tables.insert(key, table);
        Ok(())
    }
}''',
    "register table",
)

runtime = replace_once(
    runtime,
    '''    UnresolvedGlobalImport {
        module: String,
        name: String,
    },
    HostGlobalTypeMismatch {''',
    '''    UnresolvedGlobalImport {
        module: String,
        name: String,
    },
    UnresolvedTableImport {
        module: String,
        name: String,
    },
    HostTableLimitsMismatch {
        module: String,
        name: String,
        expected_minimum: u32,
        expected_maximum: Option<u32>,
        actual_minimum: u32,
        actual_maximum: Option<u32>,
    },
    HostTableAlreadyBound {
        module: String,
        name: String,
    },
    ForeignTableFunctionReference {
        element_index: u32,
    },
    HostGlobalTypeMismatch {''',
    "runtime table errors",
)

runtime = replace_once(
    runtime,
    '''            Self::UnresolvedGlobalImport { module, name } => {
                write!(f, "unresolved host global import {module}.{name}")
            }
            Self::HostGlobalTypeMismatch {''',
    '''            Self::UnresolvedGlobalImport { module, name } => {
                write!(f, "unresolved host global import {module}.{name}")
            }
            Self::UnresolvedTableImport { module, name } => {
                write!(f, "unresolved host table import {module}.{name}")
            }
            Self::HostTableLimitsMismatch {
                module,
                name,
                expected_minimum,
                expected_maximum,
                actual_minimum,
                actual_maximum,
            } => write!(
                f,
                "host table {module}.{name} has limits min={actual_minimum} max={actual_maximum:?}, which do not satisfy imported min={expected_minimum} max={expected_maximum:?}"
            ),
            Self::HostTableAlreadyBound { module, name } => write!(
                f,
                "host table {module}.{name} is already bound to another live runtime instance"
            ),
            Self::ForeignTableFunctionReference { element_index } => write!(
                f,
                "table element {element_index} refers to a function owned by another runtime instance"
            ),
            Self::HostGlobalTypeMismatch {''',
    "runtime table error display",
)

old_allocate = '''fn allocate_table(elements: u32) -> Result<Vec<Option<u32>>, RuntimeError> {
    let len =
        usize::try_from(elements).map_err(|_| RuntimeError::TableAllocationFailed { elements })?;
    let mut table = Vec::new();
    table
        .try_reserve_exact(len)
        .map_err(|_| RuntimeError::TableAllocationFailed { elements })?;
    table.resize(len, None);
    Ok(table)
}

'''
runtime = replace_once(runtime, old_allocate, "", "remove raw table allocator")

runtime = replace_once(
    runtime,
    '''pub struct Instance {
    module: Module,
    control_maps: Vec<ControlMap>,
    memory: Option<LinearMemory>,
    table: Option<Vec<Option<u32>>>,''',
    '''pub struct Instance {
    identity: Rc<()>,
    module: Module,
    control_maps: Vec<ControlMap>,
    memory: Option<LinearMemory>,
    table: Option<TableHandle>,''',
    "instance table field",
)

runtime = replace_once(
    runtime,
    '''        let table = module
            .tables
            .first()
            .map(|table_type| allocate_table(table_type.limits.min))
            .transpose()?;
        let globals = instantiate_globals(&module, &hosts)?;

        let mut instance = Self {
            module,''',
    '''        let identity = Rc::new(());
        let table = instantiate_table(&module, &hosts, &identity)?;
        let globals = instantiate_globals(&module, &hosts)?;

        let mut instance = Self {
            identity,
            module,''',
    "instantiate table",
)

runtime = replace_once(
    runtime,
    '''            let table = self
                .table
                .as_mut()
                .ok_or(RuntimeError::TableIndexOutOfBounds(0))?;
            if end > table.len() as u64 {
                return Err(RuntimeError::ElementSegmentOutOfBounds {
                    segment: segment_index,
                    offset,
                    length: segment.function_indices.len(),
                });
            }
            for (slot, &function_index) in segment.function_indices.iter().enumerate() {
                table[offset as usize + slot] = Some(function_index);
            }''',
    '''            let table = self
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
            for (slot, &function_index) in segment.function_indices.iter().enumerate() {
                let index = u32::try_from(offset + slot as u64).map_err(|_| {
                    RuntimeError::ElementSegmentOutOfBounds {
                        segment: segment_index,
                        offset,
                        length: segment.function_indices.len(),
                    }
                })?;
                table
                    .set_for_instance(index, function_index, &self.identity)
                    .map_err(|error| map_table_element_error(error, index))?;
            }''',
    "element initialization",
)

runtime = replace_once(
    runtime,
    '''                    let callee = self
                        .table
                        .as_ref()
                        .and_then(|table| table.get(element_index as usize))
                        .ok_or(RuntimeError::TableElementOutOfBounds(element_index))?
                        .ok_or(RuntimeError::UninitializedTableElement(element_index))?;''',
    '''                    let callee = self
                        .table
                        .as_ref()
                        .ok_or(RuntimeError::TableIndexOutOfBounds(table_index))?
                        .function_index_for_instance(element_index, &self.identity)
                        .map_err(|error| map_table_element_error(error, element_index))?
                        .ok_or(RuntimeError::UninitializedTableElement(element_index))?;''',
    "call_indirect table lookup",
)

runtime = replace_once(
    runtime,
    '''        match import.desc {
            ImportDesc::Function(_) | ImportDesc::Global(_) => {}
            _ => {''',
    '''        match import.desc {
            ImportDesc::Function(_) | ImportDesc::Global(_) | ImportDesc::Table(_) => {}
            _ => {''',
    "allow table runtime import",
)

validate_marker = '''    for import in &module.imports {
        let ImportDesc::Global(global_type) = import.desc else {
            continue;
        };'''
table_validation = r'''    for import in &module.imports {
        let ImportDesc::Table(table_type) = import.desc else {
            continue;
        };
        let key = (import.module.clone(), import.name.clone());
        let table = hosts
            .tables
            .get(&key)
            .ok_or_else(|| RuntimeError::UnresolvedTableImport {
                module: import.module.clone(),
                name: import.name.clone(),
            })?;
        validate_table_limits(import, table_type.limits.min, table_type.limits.max, table)?;
    }

'''
runtime = replace_once(
    runtime,
    validate_marker,
    table_validation + validate_marker,
    "validate host table bindings",
)

instantiate_globals_marker = '''fn instantiate_globals(
    module: &Module,
    hosts: &HostRegistry,
) -> Result<Vec<GlobalHandle>, RuntimeError> {'''
table_helpers = r'''fn validate_table_limits(
    import: &wasm_parser::Import,
    expected_minimum: u32,
    expected_maximum: Option<u32>,
    table: &TableHandle,
) -> Result<(), RuntimeError> {
    let actual_minimum = table.len();
    let actual_maximum = table.maximum();
    let minimum_matches = actual_minimum >= expected_minimum;
    let maximum_matches = match expected_maximum {
        None => true,
        Some(expected) => matches!(actual_maximum, Some(actual) if actual <= expected),
    };
    if minimum_matches && maximum_matches {
        return Ok(());
    }
    Err(RuntimeError::HostTableLimitsMismatch {
        module: import.module.clone(),
        name: import.name.clone(),
        expected_minimum,
        expected_maximum,
        actual_minimum,
        actual_maximum,
    })
}

fn instantiate_table(
    module: &Module,
    hosts: &HostRegistry,
    identity: &Rc<()>,
) -> Result<Option<TableHandle>, RuntimeError> {
    for import in &module.imports {
        let ImportDesc::Table(table_type) = import.desc else {
            continue;
        };
        let key = (import.module.clone(), import.name.clone());
        let table = hosts.tables.get(&key).cloned().ok_or_else(|| {
            RuntimeError::UnresolvedTableImport {
                module: import.module.clone(),
                name: import.name.clone(),
            }
        })?;
        validate_table_limits(import, table_type.limits.min, table_type.limits.max, &table)?;
        table.bind(identity).map_err(|error| match error {
            TableHandleError::AlreadyBound => RuntimeError::HostTableAlreadyBound {
                module: import.module.clone(),
                name: import.name.clone(),
            },
            other => map_table_element_error(other, 0),
        })?;
        return Ok(Some(table));
    }

    let Some(table_type) = module.tables.first() else {
        return Ok(None);
    };
    let table = TableHandle::new(table_type.limits.min, table_type.limits.max).map_err(|error| {
        match error {
            TableHandleError::AllocationFailed { elements } => {
                RuntimeError::TableAllocationFailed { elements }
            }
            TableHandleError::InvalidLimits { .. } => RuntimeError::ControlInvariant(
                "validated defined table has inconsistent limits",
            ),
            other => map_table_element_error(other, 0),
        }
    })?;
    table.bind(identity).map_err(|_| {
        RuntimeError::ControlInvariant("fresh defined table is unexpectedly already bound")
    })?;
    Ok(Some(table))
}

fn map_table_element_error(error: TableHandleError, index: u32) -> RuntimeError {
    match error {
        TableHandleError::OutOfBounds { .. } => RuntimeError::TableElementOutOfBounds(index),
        TableHandleError::ForeignFunctionReference { .. } => {
            RuntimeError::ForeignTableFunctionReference {
                element_index: index,
            }
        }
        TableHandleError::AlreadyBound => {
            RuntimeError::ControlInvariant("table binding changed while instance is live")
        }
        TableHandleError::AllocationFailed { elements } => {
            RuntimeError::TableAllocationFailed { elements }
        }
        TableHandleError::InvalidLimits { .. } => {
            RuntimeError::ControlInvariant("table handle has inconsistent limits")
        }
    }
}

'''
runtime = replace_once(
    runtime,
    instantiate_globals_marker,
    table_helpers + instantiate_globals_marker,
    "table helper insertion",
)

runtime_path.write_text(runtime)

imports_test_path = Path("crates/wasm-runtime/tests/phase5c_imports.rs")
imports_test = imports_test_path.read_text()
imports_test = replace_once(
    imports_test,
    '''        Err(RuntimeError::UnsupportedObjectImport {
            kind: ImportKind::Table,
            ..
        })''',
    '''        Err(RuntimeError::UnresolvedTableImport { .. })''',
    "existing imported table expectation",
)
imports_test_path.write_text(imports_test)

new_test = r'''use wasm_parser::parse_module;
use wasm_runtime::{
    HostRegistry, HostRegistryError, Instance, RuntimeError, TableHandle, TableHandleError, Value,
};

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

fn imported_table_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(
        &mut module,
        1,
        &[
            0x02, // two function types
            0x60, 0x00, 0x01, 0x7f, // type 0: [] -> i32
            0x60, 0x01, 0x7f, 0x01, 0x7f, // type 1: [i32] -> i32
        ],
    );

    let mut imports = vec![0x01];
    push_name(&mut imports, "env");
    push_name(&mut imports, "tab");
    imports.extend([0x01, 0x70, 0x01, 0x02, 0x04]); // table funcref min=2 max=4
    push_section(&mut module, 2, &imports);

    push_section(&mut module, 3, &[0x02, 0x00, 0x01]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x01]);
    push_section(
        &mut module,
        9,
        &[0x01, 0x00, 0x41, 0x00, 0x0b, 0x01, 0x00],
    );

    let target_body = [0x00, 0x41, 0x2a, 0x0b];
    let caller_body = [0x00, 0x20, 0x00, 0x11, 0x00, 0x00, 0x0b];
    let mut code = vec![0x02];
    push_u32(&mut code, target_body.len() as u32);
    code.extend(target_body);
    push_u32(&mut code, caller_body.len() as u32);
    code.extend(caller_body);
    push_section(&mut module, 10, &code);
    module
}

fn instantiate(table: &TableHandle) -> Instance {
    let module = parse_module(&imported_table_module()).expect("parse imported-table fixture");
    let mut hosts = HostRegistry::new();
    hosts
        .register_table("env", "tab", table.clone())
        .expect("register table");
    Instance::with_hosts(module, hosts).expect("instantiate imported-table fixture")
}

#[test]
fn active_element_initializes_shared_imported_table_and_call_indirect_executes() {
    let table = TableHandle::new(2, Some(4)).unwrap();
    let mut vm = instantiate(&table);
    assert!(table.get(0).unwrap().is_some());
    assert_eq!(
        vm.invoke_export("run", &[Value::I32(0)]).unwrap(),
        Some(Value::I32(42))
    );
}

#[test]
fn host_table_mutation_is_immediately_visible_to_call_indirect() {
    let table = TableHandle::new(2, Some(4)).unwrap();
    let mut vm = instantiate(&table);
    let target = table.get(0).unwrap().expect("element initialized slot 0");

    table.set(0, None).unwrap();
    assert!(matches!(
        vm.invoke_export("run", &[Value::I32(0)]),
        Err(RuntimeError::UninitializedTableElement(0))
    ));

    table.set(1, Some(target)).unwrap();
    assert_eq!(
        vm.invoke_export("run", &[Value::I32(1)]).unwrap(),
        Some(Value::I32(42))
    );
}

#[test]
fn imported_table_limits_follow_wasm_subtyping_rules() {
    for table in [
        TableHandle::new(1, Some(4)).unwrap(),
        TableHandle::new(2, None).unwrap(),
        TableHandle::new(2, Some(5)).unwrap(),
    ] {
        let module = parse_module(&imported_table_module()).unwrap();
        let mut hosts = HostRegistry::new();
        hosts.register_table("env", "tab", table).unwrap();
        assert!(matches!(
            Instance::with_hosts(module, hosts),
            Err(RuntimeError::HostTableLimitsMismatch { .. })
        ));
    }

    let wider_min_tighter_max = TableHandle::new(3, Some(3)).unwrap();
    let mut vm = instantiate(&wider_min_tighter_max);
    assert_eq!(
        vm.invoke_export("run", &[Value::I32(0)]).unwrap(),
        Some(Value::I32(42))
    );
}

#[test]
fn duplicate_table_registration_is_rejected() {
    let table = TableHandle::new(2, Some(4)).unwrap();
    let mut hosts = HostRegistry::new();
    hosts.register_table("env", "tab", table.clone()).unwrap();
    assert_eq!(
        hosts.register_table("env", "tab", table),
        Err(HostRegistryError::DuplicateTable {
            module: "env".into(),
            name: "tab".into(),
        })
    );
}

#[test]
fn one_table_handle_cannot_back_two_live_instances_yet() {
    let table = TableHandle::new(2, Some(4)).unwrap();
    let _first = instantiate(&table);

    let module = parse_module(&imported_table_module()).unwrap();
    let mut hosts = HostRegistry::new();
    hosts.register_table("env", "tab", table).unwrap();
    assert!(matches!(
        Instance::with_hosts(module, hosts),
        Err(RuntimeError::HostTableAlreadyBound { .. })
    ));
}

#[test]
fn stale_function_ref_never_aliases_same_numeric_index_in_new_instance() {
    let table = TableHandle::new(2, Some(4)).unwrap();
    let stale = {
        let _first = instantiate(&table);
        table.get(0).unwrap().expect("first instance writes slot 0")
    };

    let mut second = instantiate(&table);
    table.set(1, Some(stale)).unwrap();
    assert!(matches!(
        second.invoke_export("run", &[Value::I32(1)]),
        Err(RuntimeError::ForeignTableFunctionReference { element_index: 1 })
    ));
}

#[test]
fn table_handle_rejects_invalid_limits_and_oob_host_access() {
    assert_eq!(
        TableHandle::new(3, Some(2)).unwrap_err(),
        TableHandleError::InvalidLimits {
            minimum: 3,
            maximum: 2,
        }
    );
    let table = TableHandle::new(1, None).unwrap();
    assert!(matches!(
        table.get(1),
        Err(TableHandleError::OutOfBounds { index: 1, length: 1 })
    ));
}
'''
Path("crates/wasm-runtime/tests/phase5c_imported_tables.rs").write_text(new_test)
