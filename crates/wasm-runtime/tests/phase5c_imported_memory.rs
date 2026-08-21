use wasm_parser::{
    parse_module, DataSegment, Export, ExportKind, FuncType, Import, ImportDesc, Limits,
    MemoryType, Module, ValueType,
};
use wasm_runtime::{
    HostCapabilities, HostRegistry, HostRegistryError, Instance, MemoryHandle, MemoryHandleError,
    RuntimeError, RuntimeLimits, Value,
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

fn imported_memory_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(
        &mut module,
        1,
        &[
            0x03, 0x60, 0x01, 0x7f, 0x01, 0x7f, 0x60, 0x02, 0x7f, 0x7f, 0x00, 0x60, 0x00, 0x01,
            0x7f,
        ],
    );
    let mut imports = vec![0x01];
    push_name(&mut imports, "env");
    push_name(&mut imports, "mem");
    imports.extend([0x02, 0x01, 0x02, 0x04]);
    push_section(&mut module, 2, &imports);
    push_section(&mut module, 3, &[0x04, 0x00, 0x01, 0x02, 0x00]);

    let mut exports = vec![0x04];
    for (name, index) in [("load", 0u32), ("store", 1), ("size", 2), ("grow", 3)] {
        push_name(&mut exports, name);
        exports.push(0x00);
        push_u32(&mut exports, index);
    }
    push_section(&mut module, 7, &exports);

    let bodies: [&[u8]; 4] = [
        &[0x00, 0x20, 0x00, 0x28, 0x02, 0x00, 0x0b],
        &[0x00, 0x20, 0x00, 0x20, 0x01, 0x36, 0x02, 0x00, 0x0b],
        &[0x00, 0x3f, 0x00, 0x0b],
        &[0x00, 0x20, 0x00, 0x40, 0x00, 0x0b],
    ];
    let mut code = vec![bodies.len() as u8];
    for body in bodies {
        push_u32(&mut code, body.len() as u32);
        code.extend_from_slice(body);
    }
    push_section(&mut module, 10, &code);
    push_section(
        &mut module,
        11,
        &[0x01, 0x00, 0x41, 0x00, 0x0b, 0x04, b'A', b'B', b'C', b'D'],
    );
    module
}

fn imported_memory_host_callback_module() -> Module {
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
                name: "mem".into(),
                desc: ImportDesc::Memory(MemoryType {
                    limits: Limits {
                        min: 2,
                        max: Some(4),
                    },
                }),
            },
        ],
        exports: vec![Export {
            name: "touch".into(),
            kind: ExportKind::Function,
            index: 0,
        }],
        ..Module::default()
    }
}

fn instantiate(memory: &MemoryHandle) -> Instance {
    let module = parse_module(&imported_memory_module()).expect("parse imported-memory fixture");
    let mut hosts = HostRegistry::new();
    hosts.register_memory("env", "mem", memory.clone()).unwrap();
    Instance::with_hosts(module, hosts).expect("instantiate imported-memory fixture")
}

#[test]
fn data_segment_and_wasm_memory_ops_use_shared_imported_backing() {
    let memory = MemoryHandle::new(2, Some(4)).unwrap();
    let mut vm = instantiate(&memory);
    assert_eq!(memory.read(0, 4).unwrap(), b"ABCD");
    assert_eq!(
        vm.invoke_export("load", &[Value::I32(0)]).unwrap(),
        Some(Value::I32(i32::from_le_bytes(*b"ABCD")))
    );

    memory.write(8, &5i32.to_le_bytes()).unwrap();
    assert_eq!(
        vm.invoke_export("load", &[Value::I32(8)]).unwrap(),
        Some(Value::I32(5))
    );

    assert_eq!(
        vm.invoke_export("store", &[Value::I32(12), Value::I32(77)])
            .unwrap(),
        None
    );
    assert_eq!(memory.read(12, 4).unwrap(), 77i32.to_le_bytes());
}

#[test]
fn failed_data_segment_instantiation_is_atomic_for_shared_memory() {
    let memory = MemoryHandle::new(2, Some(4)).unwrap();
    memory.write(32, b"KEEP").unwrap();
    memory.write(131_071, &[0x7f]).unwrap();

    let mut module = parse_module(&imported_memory_module()).unwrap();
    module.data = vec![
        DataSegment {
            memory_index: 0,
            offset: 32,
            bytes: b"MUTATE".to_vec(),
        },
        DataSegment {
            memory_index: 0,
            offset: 131_071,
            bytes: vec![0xaa, 0xbb],
        },
    ];

    let mut hosts = HostRegistry::new();
    hosts.register_memory("env", "mem", memory.clone()).unwrap();
    let error = Instance::with_hosts(module, hosts)
        .expect_err("an out-of-bounds later segment must fail instantiation");
    assert!(matches!(
        error,
        RuntimeError::DataSegmentOutOfBounds { segment: 1, .. }
    ));
    assert_eq!(memory.read(32, 4).unwrap(), b"KEEP");
    assert_eq!(memory.read(131_071, 1).unwrap(), vec![0x7f]);
}

#[test]
fn host_callback_accesses_the_same_imported_memory_handle() {
    let memory = MemoryHandle::new(2, Some(4)).unwrap();
    memory.write(0, &123i32.to_le_bytes()).unwrap();

    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "touch",
            vec![],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ_WRITE,
            |context, _args| {
                let bytes = context.read_memory(0, 4)?;
                let value = i32::from_le_bytes(
                    bytes
                        .as_slice()
                        .try_into()
                        .expect("four-byte imported-memory read"),
                );
                context.write_memory(4, b"WASM")?;
                Ok(Some(Value::I32(value)))
            },
        )
        .unwrap();
    hosts.register_memory("env", "mem", memory.clone()).unwrap();

    let mut instance = Instance::with_hosts(imported_memory_host_callback_module(), hosts).unwrap();
    assert_eq!(
        instance.invoke_export("touch", &[]).unwrap(),
        Some(Value::I32(123))
    );
    assert_eq!(memory.read(4, 4).unwrap(), b"WASM");
}

#[test]
fn memory_grow_and_size_are_bidirectionally_visible() {
    let memory = MemoryHandle::new(2, Some(4)).unwrap();
    let mut vm = instantiate(&memory);
    assert_eq!(
        vm.invoke_export("grow", &[Value::I32(1)]).unwrap(),
        Some(Value::I32(2))
    );
    assert_eq!(memory.size_pages(), 3);
    assert_eq!(memory.grow(1), 3);
    assert_eq!(vm.invoke_export("size", &[]).unwrap(), Some(Value::I32(4)));
    assert_eq!(
        vm.invoke_export("grow", &[Value::I32(1)]).unwrap(),
        Some(Value::I32(-1))
    );
}

#[test]
fn one_memory_handle_can_back_multiple_live_instances() {
    let memory = MemoryHandle::new(2, Some(4)).unwrap();
    let mut first = instantiate(&memory);
    let mut second = instantiate(&memory);
    first
        .invoke_export("store", &[Value::I32(16), Value::I32(99)])
        .unwrap();
    assert_eq!(
        second.invoke_export("load", &[Value::I32(16)]).unwrap(),
        Some(Value::I32(99))
    );
}

#[test]
fn imported_memory_limits_follow_wasm_subtyping_and_runtime_caps() {
    for memory in [
        MemoryHandle::new(1, Some(4)).unwrap(),
        MemoryHandle::new(2, None).unwrap(),
        MemoryHandle::new(2, Some(5)).unwrap(),
    ] {
        let module = parse_module(&imported_memory_module()).unwrap();
        let mut hosts = HostRegistry::new();
        hosts.register_memory("env", "mem", memory).unwrap();
        assert!(matches!(
            Instance::with_hosts(module, hosts),
            Err(RuntimeError::HostMemoryLimitsMismatch { .. })
        ));
    }

    let compatible = MemoryHandle::new(3, Some(3)).unwrap();
    let mut vm = instantiate(&compatible);
    assert_eq!(vm.invoke_export("size", &[]).unwrap(), Some(Value::I32(3)));

    let module = parse_module(&imported_memory_module()).unwrap();
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "mem", MemoryHandle::new(2, Some(4)).unwrap())
        .unwrap();
    let limits = RuntimeLimits {
        max_memory_pages: 3,
        ..RuntimeLimits::default()
    };
    assert!(matches!(
        Instance::with_config(module, hosts, limits),
        Err(RuntimeError::HostMemoryRuntimeLimitMismatch { .. })
    ));
}

#[test]
fn unresolved_and_duplicate_memory_bindings_fail_closed() {
    let module = parse_module(&imported_memory_module()).unwrap();
    assert!(matches!(
        Instance::new(module),
        Err(RuntimeError::UnresolvedMemoryImport { .. })
    ));

    let memory = MemoryHandle::new(2, Some(4)).unwrap();
    let mut hosts = HostRegistry::new();
    hosts.register_memory("env", "mem", memory.clone()).unwrap();
    assert_eq!(
        hosts.register_memory("env", "mem", memory),
        Err(HostRegistryError::DuplicateMemory {
            module: "env".into(),
            name: "mem".into()
        })
    );
}

#[test]
fn memory_handle_rejects_bad_limits_and_oob_access() {
    assert_eq!(
        MemoryHandle::new(3, Some(2)).unwrap_err(),
        MemoryHandleError::InvalidLimits {
            minimum: 3,
            maximum: 2
        }
    );
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    assert!(matches!(
        memory.read(65535, 2),
        Err(MemoryHandleError::OutOfBounds {
            address: 65535,
            width: 2
        })
    ));
}
