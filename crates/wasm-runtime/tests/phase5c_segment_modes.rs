use wasm_parser::{
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
        Err(RuntimeError::Validation(
            ValidationError::DataMemoryOutOfBounds {
                memory_index: 1,
                ..
            }
        ))
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
fn failed_element_preflight_does_not_mutate_imported_memory() {
    let (types, function_type_indices, code) = noop_function_parts();
    let module = Module {
        types,
        imports: vec![memory_import(), table_import()],
        function_type_indices,
        code,
        data: vec![DataSegment {
            mode: DataMode::Active {
                memory_index: 0,
                offset: 8,
            },
            bytes: b"MUTATE".to_vec(),
        }],
        elements: vec![ElementSegment {
            mode: ElementMode::Active {
                table_index: 0,
                offset: 2,
            },
            function_indices: vec![0],
        }],
        ..Module::default()
    };

    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(8, b"KEEP!!").unwrap();
    let table = TableHandle::new(2, Some(2)).unwrap();
    let mut hosts = HostRegistry::new();
    hosts.register_memory("env", "mem", memory.clone()).unwrap();
    hosts.register_table("env", "tab", table.clone()).unwrap();

    let error = Instance::with_hosts(module, hosts)
        .expect_err("element OOB must fail before any imported object is mutated");
    assert!(matches!(
        error,
        RuntimeError::ElementSegmentOutOfBounds { segment: 0, .. }
    ));
    assert_eq!(memory.read(8, 6).unwrap(), b"KEEP!!");
    assert!(table.get(0).unwrap().is_none());
    assert!(table.get(1).unwrap().is_none());
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
