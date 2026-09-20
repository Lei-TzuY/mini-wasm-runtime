use wasm_parser::{
    Constant, DataMode, DataSegment, ElementMode, ElementSegment, Export, ExportKind, FuncType,
    FunctionBody, Global, GlobalType, Limits, MemoryLimits, MemoryType, Module, TableType,
    ValueType,
};
use wasm_runtime::{GlobalHandleError, Instance, RuntimeError, Value};

fn module() -> Module {
    Module {
        types: vec![FuncType {
            params: vec![],
            results: vec![],
        }],
        function_type_indices: vec![0],
        tables: vec![TableType {
            limits: Limits {
                min: 1,
                max: Some(1),
            },
        }],
        memories: vec![
            MemoryType {
                limits: MemoryLimits {
                    min: 1,
                    max: Some(1),
                    memory64: false,
                },
            },
            MemoryType {
                limits: MemoryLimits {
                    min: 1,
                    max: Some(1),
                    memory64: true,
                },
            },
        ],
        globals: vec![Global {
            ty: GlobalType {
                value_type: ValueType::I32,
                mutable: true,
            },
            init: Constant::I32(5),
        }],
        exports: vec![
            Export {
                name: "f".into(),
                kind: ExportKind::Function,
                index: 0,
            },
            Export {
                name: "tab".into(),
                kind: ExportKind::Table,
                index: 0,
            },
            Export {
                name: "m1".into(),
                kind: ExportKind::Memory,
                index: 1,
            },
            Export {
                name: "g".into(),
                kind: ExportKind::Global,
                index: 0,
            },
        ],
        elements: vec![ElementSegment {
            mode: ElementMode::Active {
                table_index: 0,
                offset: 0,
            },
            function_indices: vec![0],
        }],
        data: vec![DataSegment {
            mode: DataMode::Active {
                memory_index: 1,
                offset: 8,
            },
            bytes: b"B".to_vec(),
        }],
        code: vec![FunctionBody {
            locals: vec![],
            code: vec![0x0b],
        }],
        ..Module::default()
    }
}

#[test]
fn exported_state_lookups_resolve_live_backing() {
    let mut instance = Instance::new(module()).unwrap();

    let memory_index = instance.exported_memory_index("m1").unwrap();
    assert_eq!(memory_index, 1);
    assert_eq!(instance.read_memory_at(memory_index, 8, 1).unwrap(), b"B");
    instance.write_memory_at(memory_index, 8, b"Z").unwrap();
    assert_eq!(instance.read_memory_at(1, 8, 1).unwrap(), b"Z");

    let global = instance.exported_global("g").unwrap();
    assert_eq!(global.get(), Value::I32(5));
    global.set(Value::I32(9)).unwrap();
    assert_eq!(instance.global(0), Some(Value::I32(9)));

    let table = instance.exported_table("tab").unwrap();
    assert!(table.get(0).unwrap().is_some());
    table.set(0, None).unwrap();
    assert!(instance
        .exported_table("tab")
        .unwrap()
        .get(0)
        .unwrap()
        .is_none());
}

#[test]
fn exported_state_lookups_fail_closed_on_missing_or_wrong_kind() {
    let instance = Instance::new(module()).unwrap();

    assert!(matches!(
        instance.exported_memory_index("missing"),
        Err(RuntimeError::ExportNotFound(name)) if name == "missing"
    ));
    assert!(matches!(
        instance.exported_memory_index("g"),
        Err(RuntimeError::ExportNotMemory(name)) if name == "g"
    ));
    assert!(matches!(
        instance.exported_global("tab"),
        Err(RuntimeError::ExportNotGlobal(name)) if name == "tab"
    ));
    assert!(matches!(
        instance.exported_table("f"),
        Err(RuntimeError::ExportNotTable(name)) if name == "f"
    ));
}

#[test]
fn exported_global_preserves_handle_mutability_contract() {
    let mut module = module();
    module.globals[0].ty.mutable = false;
    let instance = Instance::new(module).unwrap();

    let global = instance.exported_global("g").unwrap();
    assert_eq!(global.set(Value::I32(7)), Err(GlobalHandleError::Immutable));
    assert_eq!(instance.global(0), Some(Value::I32(5)));
}
