use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, RuntimeError, Value};

const I32: u8 = 0x7f;

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

fn module(imported_memory: bool, result: Option<u8>, instructions: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let mut ty = vec![0x01, 0x60, 0x00];
    match result {
        Some(result) => ty.extend([0x01, result]),
        None => ty.push(0x00),
    }
    push_section(&mut bytes, 1, &ty);
    if imported_memory {
        let mut imports = vec![0x01];
        push_name(&mut imports, "env");
        push_name(&mut imports, "mem");
        imports.extend([0x02, 0x01, 0x01, 0x01]);
        push_section(&mut bytes, 2, &imports);
    }
    push_section(&mut bytes, 3, &[0x01, 0x00]);
    if !imported_memory {
        push_section(&mut bytes, 5, &[0x01, 0x01, 0x01, 0x01]);
    }
    push_section(&mut bytes, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);
    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend(body);
    push_section(&mut bytes, 10, &code);
    bytes
}

#[test]
fn memory_fill_uses_low_byte_of_value() {
    let bytes = module(
        false,
        Some(I32),
        &[
            0x41, 0x03, 0x41, 0x7f, 0x41, 0x04, 0xfc, 0x0b, 0x00, 0x41, 0x06, 0x2d, 0x00, 0x00,
        ],
    );
    let mut instance = Instance::new(parse_module(&bytes).unwrap()).unwrap();
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(255))
    );
}

#[test]
fn memory_fill_preflights_destination_before_mutation() {
    let bytes = module(
        true,
        None,
        &[
            0x41, 0xff, 0xff, 0x03, 0x41, 0x2a, 0x41, 0x02, 0xfc, 0x0b, 0x00,
        ],
    );
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(65_535, &[0x11]).unwrap();
    let mut hosts = HostRegistry::new();
    hosts.register_memory("env", "mem", memory.clone()).unwrap();
    let mut instance = Instance::with_hosts(parse_module(&bytes).unwrap(), hosts).unwrap();
    assert!(matches!(
        instance.invoke_export("run", &[]),
        Err(RuntimeError::MemoryOutOfBounds { .. })
    ));
    assert_eq!(memory.read(65_535, 1).unwrap(), vec![0x11]);
}

#[test]
fn memory_fill_updates_imported_memory_backing() {
    let bytes = module(
        true,
        None,
        &[0x41, 0x08, 0x41, 0x2a, 0x41, 0x03, 0xfc, 0x0b, 0x00],
    );
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let mut hosts = HostRegistry::new();
    hosts.register_memory("env", "mem", memory.clone()).unwrap();
    let mut instance = Instance::with_hosts(parse_module(&bytes).unwrap(), hosts).unwrap();
    instance.invoke_export("run", &[]).unwrap();
    assert_eq!(memory.read(8, 3).unwrap(), vec![0x2a; 3]);
}

#[test]
fn memory_fill_rejects_nonzero_memory_index() {
    let bytes = module(
        false,
        None,
        &[0x41, 0x00, 0x41, 0x00, 0x41, 0x00, 0xfc, 0x0b, 0x01],
    );
    assert!(Instance::new(parse_module(&bytes).unwrap()).is_err());
}
