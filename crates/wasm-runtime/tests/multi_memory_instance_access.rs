use wasm_parser::{
    DataMode, DataSegment, Import, ImportDesc, MemoryLimits, MemoryType, Module,
};
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, RuntimeError};

fn memory32(min: u64, max: u64) -> MemoryType {
    MemoryType {
        limits: MemoryLimits {
            min,
            max: Some(max),
            memory64: false,
        },
    }
}

fn memory64(min: u64, max: u64) -> MemoryType {
    MemoryType {
        limits: MemoryLimits {
            min,
            max: Some(max),
            memory64: true,
        },
    }
}

fn mixed_defined_module() -> Module {
    Module {
        memories: vec![memory32(1, 1), memory64(1, 1)],
        data: vec![
            DataSegment {
                mode: DataMode::Active {
                    memory_index: 0,
                    offset: 8,
                },
                bytes: b"A".to_vec(),
            },
            DataSegment {
                mode: DataMode::Active {
                    memory_index: 1,
                    offset: 8,
                },
                bytes: b"B".to_vec(),
            },
        ],
        ..Module::default()
    }
}

#[test]
fn instance_embedding_can_access_mixed_width_memories_by_index() {
    let mut instance = Instance::new(mixed_defined_module()).unwrap();

    assert_eq!(instance.memory_count(), 2);
    assert_eq!(instance.memory_size_pages_at(0).unwrap(), 1);
    assert_eq!(instance.memory_size_pages_at(1).unwrap(), 1);
    assert_eq!(instance.read_memory_at(0, 8, 1).unwrap(), b"A");
    assert_eq!(instance.read_memory_at(1, 8, 1).unwrap(), b"B");

    instance.write_memory_at(1, 8, b"Z").unwrap();

    assert_eq!(instance.read_memory_at(0, 8, 1).unwrap(), b"A");
    assert_eq!(instance.read_memory_at(1, 8, 1).unwrap(), b"Z");
    assert_eq!(instance.memory().unwrap().bytes()[8], b'A');
}

#[test]
fn instance_embedding_preserves_imported_memory_aliasing_at_nonzero_index() {
    let module = Module {
        imports: vec![
            Import {
                module: "env".into(),
                name: "m0".into(),
                desc: ImportDesc::Memory(memory32(1, 1)),
            },
            Import {
                module: "env".into(),
                name: "m1".into(),
                desc: ImportDesc::Memory(memory64(1, 1)),
            },
        ],
        ..Module::default()
    };

    let memory0 = MemoryHandle::new(1, Some(1)).unwrap();
    let memory1 = MemoryHandle::new64(1, Some(1)).unwrap();
    memory0.write(8, b"A").unwrap();
    memory1.write(8, b"B").unwrap();

    let mut hosts = HostRegistry::new();
    hosts.register_memory("env", "m0", memory0.clone()).unwrap();
    hosts.register_memory("env", "m1", memory1.clone()).unwrap();

    let mut instance = Instance::with_hosts(module, hosts).unwrap();
    assert_eq!(instance.memory_count(), 2);
    assert_eq!(instance.read_memory_at(1, 8, 1).unwrap(), b"B");

    instance.write_memory_at(1, 8, b"Z").unwrap();

    assert_eq!(memory0.read(8, 1).unwrap(), b"A");
    assert_eq!(memory1.read(8, 1).unwrap(), b"Z");
}

#[test]
fn instance_embedding_index_and_u64_bounds_fail_closed_without_partial_write() {
    let mut instance = Instance::new(mixed_defined_module()).unwrap();

    assert!(matches!(
        instance.read_memory_at(2, 0, 1),
        Err(RuntimeError::MemoryIndexOutOfBounds(2))
    ));

    let large_address = u64::from(u32::MAX) + 1;
    assert!(matches!(
        instance.write_memory_at(1, large_address, b"X"),
        Err(RuntimeError::MemoryOutOfBounds {
            address,
            width: 1,
        }) if address == large_address
    ));

    assert_eq!(instance.read_memory_at(1, 8, 1).unwrap(), b"B");
}
