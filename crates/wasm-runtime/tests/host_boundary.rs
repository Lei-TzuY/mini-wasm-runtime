use wasm_parser::{
    DataMode, DataSegment, Export, ExportKind, FuncType, FunctionBody, Import, ImportDesc, Limits,
    MemoryType, Module, ValueType,
};
use wasm_runtime::{
    HostCapabilities, HostError, HostRegistry, Instance, RuntimeError, RuntimeLimits, Value,
    MAX_CALL_DEPTH,
};

fn imported_reader_module() -> Module {
    Module {
        types: vec![FuncType {
            params: vec![],
            results: vec![ValueType::I32],
        }],
        imports: vec![Import {
            module: "env".into(),
            name: "read_first".into(),
            desc: ImportDesc::Function(0),
        }],
        memories: vec![MemoryType {
            limits: Limits {
                min: 1,
                max: Some(1),
            },
        }],
        exports: vec![Export {
            name: "read_first".into(),
            kind: ExportKind::Function,
            index: 0,
        }],
        data: vec![DataSegment {
            mode: DataMode::Active {
                memory_index: 0,
                offset: 0,
            },
            bytes: b"wasm".to_vec(),
        }],
        ..Module::default()
    }
}

#[test]
fn host_memory_read_requires_explicit_capability() {
    let mut denied = HostRegistry::new();
    denied
        .register(
            "env",
            "read_first",
            vec![],
            vec![ValueType::I32],
            HostCapabilities::NONE,
            |ctx, _args| {
                let bytes = ctx.read_memory(0, 4)?;
                Ok(Some(Value::I32(i32::from(bytes[0]))))
            },
        )
        .unwrap();
    let mut instance = Instance::with_hosts(imported_reader_module(), denied).unwrap();
    assert!(matches!(
        instance.invoke_export("read_first", &[]),
        Err(RuntimeError::HostCallFailed {
            error: HostError::CapabilityDenied("memory.read"),
            ..
        })
    ));

    let mut allowed = HostRegistry::new();
    allowed
        .register(
            "env",
            "read_first",
            vec![],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ,
            |ctx, _args| {
                let bytes = ctx.read_memory(0, 4)?;
                Ok(Some(Value::I32(i32::from(bytes[0]))))
            },
        )
        .unwrap();
    let mut instance = Instance::with_hosts(imported_reader_module(), allowed).unwrap();
    assert_eq!(
        instance.invoke_export("read_first", &[]).unwrap(),
        Some(Value::I32(i32::from(b'w')))
    );
}

#[test]
fn host_result_arity_is_checked_after_callback() {
    let module = imported_reader_module();
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "read_first",
            vec![],
            vec![ValueType::I32],
            HostCapabilities::NONE,
            |_ctx, _args| Ok(None),
        )
        .unwrap();
    let mut instance = Instance::with_hosts(module, hosts).unwrap();
    assert!(matches!(
        instance.invoke_export("read_first", &[]),
        Err(RuntimeError::HostResultArityMismatch {
            expected: 1,
            actual: 0,
            ..
        })
    ));
}

#[test]
fn configured_call_depth_stops_recursive_wasm() {
    let module = Module {
        types: vec![FuncType {
            params: vec![],
            results: vec![],
        }],
        function_type_indices: vec![0],
        exports: vec![Export {
            name: "recurse".into(),
            kind: ExportKind::Function,
            index: 0,
        }],
        code: vec![FunctionBody {
            locals: vec![],
            code: vec![0x10, 0x00, 0x0b],
        }],
        ..Module::default()
    };
    let limits = RuntimeLimits {
        max_call_depth: 2,
        ..RuntimeLimits::default()
    };
    let mut instance = Instance::with_config(module, HostRegistry::new(), limits).unwrap();
    assert!(matches!(
        instance.invoke_export("recurse", &[]),
        Err(RuntimeError::CallDepthExceeded { limit: 2 })
    ));
}

#[test]
fn configured_call_depth_cannot_bypass_runtime_safety_ceiling() {
    let module = Module {
        types: vec![FuncType {
            params: vec![],
            results: vec![],
        }],
        function_type_indices: vec![0],
        exports: vec![Export {
            name: "recurse".into(),
            kind: ExportKind::Function,
            index: 0,
        }],
        code: vec![FunctionBody {
            locals: vec![],
            code: vec![0x10, 0x00, 0x0b],
        }],
        ..Module::default()
    };
    let limits = RuntimeLimits {
        max_call_depth: usize::MAX,
        ..RuntimeLimits::default()
    };
    let mut instance = Instance::with_config(module, HostRegistry::new(), limits).unwrap();
    assert!(matches!(
        instance.invoke_export("recurse", &[]),
        Err(RuntimeError::CallDepthExceeded {
            limit: MAX_CALL_DEPTH
        })
    ));
}
