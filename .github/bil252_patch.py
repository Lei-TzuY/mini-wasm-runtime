from pathlib import Path


def rep(text, old, new, label):
    n = text.count(old)
    if n != 1:
        raise SystemExit(f"{label}: expected 1 match, got {n}")
    return text.replace(old, new, 1)


p = Path("crates/wasm-runtime/src/lib.rs")
s = p.read_text()
s = rep(s,
'''#[derive(Debug)]
pub struct Instance {
    identity: Rc<()>,
    module: Module,
    control_maps: Vec<ControlMap>,
    memory: Option<LinearMemory>,
    imported_memory: Option<MemoryHandle>,
    data_segments: Vec<Vec<u8>>,
''',
'''#[derive(Debug)]
enum RuntimeMemory {
    Owned(LinearMemory),
    Imported(MemoryHandle),
}

#[derive(Debug)]
pub struct Instance {
    identity: Rc<()>,
    module: Module,
    control_maps: Vec<ControlMap>,
    memories: Vec<RuntimeMemory>,
    data_segments: Vec<Vec<u8>>,
''', "instance storage")
s = rep(s,
'''        let imported_memory = instantiate_imported_memory(&module, &hosts, limits)?;
        let memory = if imported_memory.is_none() {
            module
                .memories
                .first()
                .map(|memory_type| {
                    LinearMemory::new(
                        memory_type.limits.min,
                        memory_type.limits.max,
                        limits.max_memory_pages,
                    )
                })
                .transpose()?
        } else {
            None
        };
''',
'''        let memories = instantiate_memories(&module, &hosts, limits)?;
''', "construct memories")
s = rep(s,
'''            module,
            control_maps,
            memory,
            imported_memory,
            data_segments,
''',
'''            module,
            control_maps,
            memories,
            data_segments,
''', "instance init")
s = rep(s,
'''    pub fn memory(&self) -> Option<&LinearMemory> {
        self.memory.as_ref()
    }
''',
'''    pub fn memory(&self) -> Option<&LinearMemory> {
        match self.memories.first() {
            Some(RuntimeMemory::Owned(memory)) => Some(memory),
            Some(RuntimeMemory::Imported(_)) | None => None,
        }
    }
''', "memory accessor")

start = s.index("    fn initialize_data_segments(&mut self) -> Result<(), RuntimeError> {")
end = s.index("    fn memory_init(\n", start)
s = s[:start] + '''    fn initialize_data_segments(&mut self) -> Result<(), RuntimeError> {
        let data = self.module.data.clone();
        for (segment_index, segment) in data.iter().enumerate() {
            let DataMode::Active { memory_index, offset } = segment.mode else { continue; };
            let offset = u64::from(offset as u32);
            let end = offset.checked_add(segment.bytes.len() as u64).ok_or(
                RuntimeError::DataSegmentOutOfBounds { segment: segment_index, offset, length: segment.bytes.len() },
            )?;
            let memory_len = self.with_memory_index(memory_index, |memory| Ok(memory.bytes.len() as u64))?;
            if end > memory_len {
                return Err(RuntimeError::DataSegmentOutOfBounds {
                    segment: segment_index,
                    offset,
                    length: segment.bytes.len(),
                });
            }
        }
        for segment in &data {
            let DataMode::Active { memory_index, offset } = segment.mode else { continue; };
            let offset = u64::from(offset as u32);
            self.with_memory_index_mut(memory_index, |memory| {
                let start = usize::try_from(offset).map_err(|_| RuntimeError::ControlInvariant(
                    "preflighted data offset no longer fits usize",
                ))?;
                let end = start + segment.bytes.len();
                memory.bytes[start..end].copy_from_slice(&segment.bytes);
                Ok(())
            })?;
        }
        Ok(())
    }

''' + s[end:]
s = rep(s,
'''    fn memory_init(
        &mut self,
        data_index: u32,
        destination: i32,
''',
'''    fn memory_init(
        &mut self,
        data_index: u32,
        memory_index: u32,
        destination: i32,
''', "memory_init signature")
s = rep(s,
'''        self.with_memory_mut(|memory| {
            let range = memory.checked_range(destination, 0, width)?;
            memory.bytes[range].copy_from_slice(&payload);
            Ok(())
        })
    }

    fn data_drop''',
'''        self.with_memory_index_mut(memory_index, |memory| {
            let range = memory.checked_range(destination, 0, width)?;
            memory.bytes[range].copy_from_slice(&payload);
            Ok(())
        })
    }

    fn memory_copy(
        &mut self,
        destination_memory: u32,
        source_memory: u32,
        destination: i32,
        source: i32,
        length: i32,
    ) -> Result<(), RuntimeError> {
        let width = length as u32 as usize;
        let payload = self.with_memory_index(source_memory, |memory| {
            let range = memory.checked_range(source, 0, width)?;
            Ok(memory.bytes[range].to_vec())
        })?;
        self.with_memory_index_mut(destination_memory, |memory| {
            let range = memory.checked_range(destination, 0, width)?;
            memory.bytes[range].copy_from_slice(&payload);
            Ok(())
        })
    }

    fn data_drop''', "memory_init/copy")

start = s.index("    fn with_memory<R>(\n")
end = s.index("    fn function_type(\n", start)
s = s[:start] + '''    fn with_memory_index<R>(
        &self,
        index: u32,
        f: impl FnOnce(&LinearMemory) -> Result<R, RuntimeError>,
    ) -> Result<R, RuntimeError> {
        match self.memories.get(index as usize) {
            Some(RuntimeMemory::Owned(memory)) => f(memory),
            Some(RuntimeMemory::Imported(memory)) => {
                let memory = memory.memory.borrow();
                f(&memory)
            }
            None => Err(RuntimeError::MemoryIndexOutOfBounds(index)),
        }
    }

    fn with_memory_index_mut<R>(
        &mut self,
        index: u32,
        f: impl FnOnce(&mut LinearMemory) -> Result<R, RuntimeError>,
    ) -> Result<R, RuntimeError> {
        match self.memories.get_mut(index as usize) {
            Some(RuntimeMemory::Owned(memory)) => f(memory),
            Some(RuntimeMemory::Imported(memory)) => {
                let mut memory = memory.memory.borrow_mut();
                f(&mut memory)
            }
            None => Err(RuntimeError::MemoryIndexOutOfBounds(index)),
        }
    }

    fn with_memory<R>(
        &self,
        f: impl FnOnce(&LinearMemory) -> Result<R, RuntimeError>,
    ) -> Result<R, RuntimeError> {
        if self.memories.is_empty() { return Err(RuntimeError::MemoryUnavailable); }
        self.with_memory_index(0, f)
    }

    fn with_memory_mut<R>(
        &mut self,
        f: impl FnOnce(&mut LinearMemory) -> Result<R, RuntimeError>,
    ) -> Result<R, RuntimeError> {
        if self.memories.is_empty() { return Err(RuntimeError::MemoryUnavailable); }
        self.with_memory_index_mut(0, f)
    }

''' + s[end:]
s = rep(s,
'''        let key = (import.module.clone(), import.name.clone());
        let (hosts, memory, imported_memory) =
            (&mut self.hosts, &mut self.memory, &self.imported_memory);
        let host = hosts
''',
'''        let key = (import.module.clone(), import.name.clone());
        let (hosts, memories) = (&mut self.hosts, &mut self.memories);
        let host = hosts
''', "host split")
s = rep(s,
'''        let context_memory = if let Some(shared) = imported_memory.as_ref() {
            Some(HostMemory::Shared(shared.clone()))
        } else {
            memory.as_mut().map(HostMemory::Owned)
        };
''',
'''        let context_memory = match memories.first_mut() {
            Some(RuntimeMemory::Owned(memory)) => Some(HostMemory::Owned(memory)),
            Some(RuntimeMemory::Imported(memory)) => Some(HostMemory::Shared(memory.clone())),
            None => None,
        };
''', "host memory zero")
s = rep(s,
'''                    stack.push(Value::I32(
                        self.with_memory(|memory| Ok(memory.size_pages()))? as i32,
                    ));
''',
'''                    stack.push(Value::I32(
                        self.with_memory_index(memory_index, |memory| Ok(memory.size_pages()))? as i32,
                    ));
''', "memory.size")
s = rep(s,
'''                    let previous = self.with_memory_mut(|memory| Ok(memory.grow(delta)))?;
''',
'''                    let previous = self
                        .with_memory_index_mut(memory_index, |memory| Ok(memory.grow(delta)))?;
''', "memory.grow")
s = rep(s,
'''                            self.memory_init(data_index, destination, source, length)?;
''',
'''                            self.memory_init(data_index, memory_index, destination, source, length)?;
''', "memory.init")
s = rep(s,
'''                            self.with_memory_mut(|memory| {
                                memory.copy(destination, source, length)
                            })?;
''',
'''                            self.memory_copy(
                                destination_memory,
                                source_memory,
                                destination,
                                source,
                                length,
                            )?;
''', "memory.copy")
s = rep(s,
'''                            self.with_memory_mut(|memory| memory.fill(destination, value, length))?;
''',
'''                            self.with_memory_index_mut(memory_index, |memory| {
                                memory.fill(destination, value, length)
                            })?;
''', "memory.fill")
start = s.index("fn instantiate_imported_memory(\n")
end = s.index("fn validate_table_limits(\n", start)
s = s[:start] + '''fn instantiate_memories(
    module: &Module,
    hosts: &HostRegistry,
    limits: RuntimeLimits,
) -> Result<Vec<RuntimeMemory>, RuntimeError> {
    let mut memories = Vec::with_capacity(module.memory_count());
    for import in &module.imports {
        let ImportDesc::Memory(memory_type) = import.desc else { continue; };
        let key = (import.module.clone(), import.name.clone());
        let memory = hosts.memories.get(&key).cloned().ok_or_else(|| RuntimeError::UnresolvedMemoryImport {
            module: import.module.clone(),
            name: import.name.clone(),
        })?;
        validate_memory_limits(import, memory_type.limits.min, memory_type.limits.max, &memory)?;
        validate_memory_runtime_limit(import, &memory, limits.max_memory_pages)?;
        memories.push(RuntimeMemory::Imported(memory));
    }
    for memory_type in &module.memories {
        memories.push(RuntimeMemory::Owned(LinearMemory::new(
            memory_type.limits.min,
            memory_type.limits.max,
            limits.max_memory_pages,
        )?));
    }
    Ok(memories)
}

''' + s[end:]
s = rep(s,
'''fn ensure_runtime_memory_index(instance: &Instance, index: u32) -> Result<(), RuntimeError> {
    if index != 0 || (instance.memory.is_none() && instance.imported_memory.is_none()) {
        Err(RuntimeError::MemoryIndexOutOfBounds(index))
    } else {
        Ok(())
    }
}
''',
'''fn ensure_runtime_memory_index(instance: &Instance, index: u32) -> Result<(), RuntimeError> {
    if instance.memories.get(index as usize).is_some() {
        Ok(())
    } else {
        Err(RuntimeError::MemoryIndexOutOfBounds(index))
    }
}
''', "runtime memory bounds")
p.write_text(s)

p = Path("crates/wasm-validator/src/lib.rs")
s = p.read_text()
s = rep(s,
'''fn validate_memories(module: &Module) -> Result<(), ValidationError> {
    if module.memory_count() > 1 {
        return Err(ValidationError::UnsupportedMemoryCount {
            count: module.memory_count(),
        });
    }

    for memory in 0..module.memory_count() {
''',
'''fn validate_memories(module: &Module) -> Result<(), ValidationError> {
    for memory in 0..module.memory_count() {
''', "validator gate")
s = rep(s,
'''    #[test]
    fn rejects_multiple_memories() {
        let mut module = valid_module();
        module.memories = vec![
            MemoryType {
                limits: Limits { min: 1, max: None },
            },
            MemoryType {
                limits: Limits { min: 1, max: None },
            },
        ];
        assert_eq!(
            validate(&module),
            Err(ValidationError::UnsupportedMemoryCount { count: 2 })
        );
    }
''',
'''    #[test]
    fn accepts_multiple_memories() {
        let mut module = valid_module();
        module.memories = vec![
            MemoryType { limits: Limits { min: 1, max: None } },
            MemoryType { limits: Limits { min: 1, max: None } },
        ];
        assert_eq!(validate(&module), Ok(()));
    }
''', "validator regression")
p.write_text(s)

p = Path("crates/wasm-runtime/tests/phase5c_imports.rs")
s = p.read_text()
s = rep(s,
'''#[test]
fn imported_and_defined_memory_still_obey_single_memory_runtime_subset() {
    let mut module = module_header();
    let mut imports = vec![0x01];
    push_import_prefix(&mut imports, "env", "mem", 0x02);
    imports.extend([0x00, 0x01]);
    push_section(&mut module, 2, &imports);
    push_section(&mut module, 5, &[0x01, 0x00, 0x01]);

    let parsed = parse_module(&module).unwrap();
    assert_eq!(parsed.memory_count(), 2);
    assert_eq!(
        validate(&parsed),
        Err(ValidationError::UnsupportedMemoryCount { count: 2 })
    );
}
''',
'''#[test]
fn imported_and_defined_memories_share_one_index_space() {
    let mut module = module_header();
    let mut imports = vec![0x01];
    push_import_prefix(&mut imports, "env", "mem", 0x02);
    imports.extend([0x00, 0x01]);
    push_section(&mut module, 2, &imports);
    push_section(&mut module, 5, &[0x01, 0x00, 0x01]);

    let parsed = parse_module(&module).unwrap();
    assert_eq!(parsed.memory_count(), 2);
    assert_eq!(validate(&parsed), Ok(()));
}
''', "imports expectation")
p.write_text(s)

Path("crates/wasm-runtime/tests/multi_memory.rs").write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, RuntimeError, Value};
use wasm_validator::ValidationError;

fn u32leb(out: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut b = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 { b |= 0x80; }
        out.push(b);
        if value == 0 { break; }
    }
}
fn name(out: &mut Vec<u8>, s: &str) { u32leb(out, s.len() as u32); out.extend_from_slice(s.as_bytes()); }
fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) { module.push(id); u32leb(module, payload.len() as u32); module.extend_from_slice(payload); }
fn header() -> Vec<u8> { b"\0asm\x01\0\0\0".to_vec() }

fn two_defined_size(index: u8) -> Vec<u8> {
    let mut m = header();
    section(&mut m, 1, &[1, 0x60, 0, 1, 0x7f]);
    section(&mut m, 3, &[1, 0]);
    section(&mut m, 5, &[2, 1, 1, 2, 1, 2, 3]);
    section(&mut m, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);
    section(&mut m, 10, &[1, 4, 0, 0x3f, index, 0x0b]);
    m
}

fn imported_defined_copy() -> Vec<u8> {
    let mut m = header();
    section(&mut m, 1, &[1, 0x60, 0, 0]);
    let mut imports = vec![1];
    name(&mut imports, "env"); name(&mut imports, "mem"); imports.extend([2, 0, 1]);
    section(&mut m, 2, &imports);
    section(&mut m, 3, &[1, 0]);
    section(&mut m, 5, &[1, 0, 1]);
    section(&mut m, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);
    section(&mut m, 10, &[1, 12, 0, 0x41, 0, 0x41, 0, 0x41, 4, 0xfc, 10, 0, 1, 0x0b]);
    let mut data = vec![1, 2, 1, 0x41, 0, 0x0b, 4]; data.extend_from_slice(b"wasm");
    section(&mut m, 11, &data);
    m
}

#[test]
fn memory_size_executes_against_nonzero_defined_memory() {
    let module = parse_module(&two_defined_size(1)).unwrap();
    let mut vm = Instance::new(module).unwrap();
    assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(2)));
}

#[test]
fn imported_memory_precedes_defined_memory_and_cross_copy_executes() {
    let module = parse_module(&imported_defined_copy()).unwrap();
    assert_eq!(module.memory_count(), 2);
    let memory = MemoryHandle::new(1, None).unwrap();
    let mut hosts = HostRegistry::new();
    hosts.register_memory("env", "mem", memory.clone()).unwrap();
    let mut vm = Instance::with_hosts(module, hosts).unwrap();
    assert_eq!(memory.read(0, 4).unwrap(), vec![0, 0, 0, 0]);
    vm.invoke_export("run", &[]).unwrap();
    assert_eq!(memory.read(0, 4).unwrap(), b"wasm");
}

#[test]
fn invalid_memory_index_remains_fail_closed() {
    let module = parse_module(&two_defined_size(2)).unwrap();
    assert!(matches!(Instance::new(module), Err(RuntimeError::Validation(ValidationError::MemoryIndexOutOfBounds { memory_index: 2, .. }))));
}
''')
