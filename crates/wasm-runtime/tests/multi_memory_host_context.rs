use wasm_parser::{
    DataMode, DataSegment, Export, ExportKind, FuncType, FunctionBody, Import, ImportDesc,
    MemoryLimits, MemoryType, Module, ValueType,
};
use wasm_runtime::{
    HostCapabilities, HostError, HostRegistry, Instance, MemoryHandle, RuntimeError, Value,
};

fn memory_type(min: u64, max: u64) -> MemoryType {
    MemoryType {
        limits: MemoryLimits {
            min,
            max: Some(max),
            memory64: false,
        },
    }
}

fn defined_two_memory_module() -> Module {
    Module {
        types: vec![FuncType {
            params: vec![],
            results: vec![ValueType::I32],
        }],
        imports: vec![Import {
            module: "env".into(),
            name: "touch".into(),
            desc: ImportDesc::Function(0),
        }],
        function_type_indices: vec![0],
        memories: vec![memory_type(1, 1), memory_type(2, 2)],
        exports: vec![Export {
            name: "run".into(),
            kind: ExportKind::Function,
            index: 1,
        }],
        code: vec![FunctionBody {
            locals: vec![],
            code: vec![0x10, 0x00, 0x0b],
        }],
        data: vec![
            DataSegment {
                mode: DataMode::Active {
                    memory_index: 0,
                    offset: 0,
                },
                bytes: b"A".to_vec(),
            },
            DataSegment {
                mode: DataMode::Active {
                    memory_index: 1,
                    offset: 0,
                },
                bytes: b"B".to_vec(),
            },
        ],
        ..Module::default()
    }
}

fn imported_two_memory_module() -> Module {
    Module {
        types: vec![FuncType {
            params: vec![],
            results: vec![ValueType::I32],
        }],
        imports: vec![
            Import {
                module: "env".into(),
                name: "touch".into(),
                desc: ImportDesc::Function(0),
            },
            Import {
                module: "env".into(),
                name: "m0".into(),
                desc: ImportDesc::Memory(memory_type(1, 1)),
            },
            Import {
                module: "env".into(),
                name: "m1".into(),
                desc: ImportDesc::Memory(memory_type(1, 1)),
            },
        ],
        function_type_indices: vec![0],
        exports: vec![Export {
            name: "run".into(),
            kind: ExportKind::Function,
            index: 1,
        }],
        code: vec![FunctionBody {
            locals: vec![],
            code: vec![0x10, 0x00, 0x0b],
        }],
        ..Module::default()
    }
}

fn indexed_touch_hosts() -> HostRegistry {
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "touch",
            vec![],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ_WRITE,
            |ctx, _args| {
                assert_eq!(ctx.memory_count(), 2);
                assert_eq!(ctx.memory_size_pages()?, 1);
                assert_eq!(ctx.memory_size_pages_at(1)?, 2);
                assert_eq!(ctx.read_memory(0, 1)?, b"A");
                let before = ctx.read_memory_at(1, 0, 1)?;
                assert_eq!(before, b"B");
                ctx.write_memory_at(1, 0, b"Z")?;
                let after = ctx.read_memory_at(1, 0, 1)?;
                Ok(Some(Value::I32(
                    (i32::from(before[0]) << 8) | i32::from(after[0]),
                )))
            },
        )
        .unwrap();
    hosts
}

#[test]
fn host_context_can_access_second_defined_memory_without_retargeting_legacy_helpers() {
    let mut instance =
        Instance::with_hosts(defined_two_memory_module(), indexed_touch_hosts()).unwrap();

    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32((i32::from(b'B') << 8) | i32::from(b'Z')))
    );
    assert_eq!(instance.memory().unwrap().bytes()[0], b'A');
}

#[test]
fn host_context_indexed_access_preserves_imported_memory_aliasing() {
    let memory0 = MemoryHandle::new(1, Some(1)).unwrap();
    let memory1 = MemoryHandle::new(1, Some(1)).unwrap();
    memory0.write(0, b"A").unwrap();
    memory1.write(0, b"B").unwrap();

    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "touch",
            vec![],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ_WRITE,
            |ctx, _args| {
                assert_eq!(ctx.memory_count(), 2);
                let before = ctx.read_memory_at(1, 0, 1)?;
                ctx.write_memory_at(1, 0, b"Z")?;
                let after = ctx.read_memory_at(1, 0, 1)?;
                Ok(Some(Value::I32(
                    (i32::from(before[0]) << 8) | i32::from(after[0]),
                )))
            },
        )
        .unwrap();
    hosts
        .register_memory("env", "m0", memory0.clone())
        .unwrap();
    hosts
        .register_memory("env", "m1", memory1.clone())
        .unwrap();

    let mut instance = Instance::with_hosts(imported_two_memory_module(), hosts).unwrap();
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32((i32::from(b'B') << 8) | i32::from(b'Z')))
    );
    assert_eq!(memory0.read(0, 1).unwrap(), b"A");
    assert_eq!(memory1.read(0, 1).unwrap(), b"Z");
}

#[test]
fn indexed_host_memory_access_fails_closed_for_capability_and_index_errors() {
    let mut denied = HostRegistry::new();
    denied
        .register(
            "env",
            "touch",
            vec![],
            vec![ValueType::I32],
            HostCapabilities::NONE,
            |ctx, _args| {
                let _ = ctx.read_memory_at(99, 0, 1)?;
                Ok(Some(Value::I32(0)))
            },
        )
        .unwrap();
    let mut instance = Instance::with_hosts(defined_two_memory_module(), denied).unwrap();
    assert!(matches!(
        instance.invoke_export("run", &[]),
        Err(RuntimeError::HostCallFailed {
            error: HostError::CapabilityDenied("memory.read"),
            ..
        })
    ));

    let mut invalid_index = HostRegistry::new();
    invalid_index
        .register(
            "env",
            "touch",
            vec![],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ,
            |ctx, _args| {
                let _ = ctx.read_memory_at(99, 0, 1)?;
                Ok(Some(Value::I32(0)))
            },
        )
        .unwrap();
    let mut instance =
        Instance::with_hosts(defined_two_memory_module(), invalid_index).unwrap();
    assert!(matches!(
        instance.invoke_export("run", &[]),
        Err(RuntimeError::HostCallFailed {
            error: HostError::MemoryUnavailable,
            ..
        })
    ));
}
