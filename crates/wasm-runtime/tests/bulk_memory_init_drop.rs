use wasm_parser::{parse_module, ParseError};
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, RuntimeError};
use wasm_validator::ValidationError;

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

fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn module(
    imported_memory: bool,
    body: &[u8],
    data: &[u8],
    declared_count: Option<u8>,
) -> Vec<u8> {
    let mut wasm = b"\0asm\x01\0\0\0".to_vec();
    section(&mut wasm, 1, &[1, 0x60, 0, 0]);
    if imported_memory {
        let mut imports = vec![1];
        push_name(&mut imports, "env");
        push_name(&mut imports, "mem");
        imports.extend([0x02, 0x01, 0x01, 0x01]);
        section(&mut wasm, 2, &imports);
    }
    section(&mut wasm, 3, &[1, 0]);
    if !imported_memory {
        section(&mut wasm, 5, &[1, 0, 1]);
    }
    section(&mut wasm, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);
    if let Some(declared_count) = declared_count {
        section(&mut wasm, 12, &[declared_count]);
    }
    let mut code = vec![1, (body.len() + 1) as u8, 0];
    code.extend_from_slice(body);
    section(&mut wasm, 10, &code);
    let mut data_section = vec![1, 1, data.len() as u8];
    data_section.extend_from_slice(data);
    section(&mut wasm, 11, &data_section);
    wasm
}

#[test]
fn memory_init_copies_passive_data() {
    let body = [0x41, 4, 0x41, 1, 0x41, 3, 0xfc, 8, 0, 0, 0x0b];
    let parsed = parse_module(&module(false, &body, b"hello", Some(1))).unwrap();
    let mut vm = Instance::new(parsed).unwrap();
    vm.invoke_export("run", &[]).unwrap();
    assert_eq!(&vm.memory().unwrap().bytes()[4..7], b"ell");
}

#[test]
fn memory_init_updates_imported_memory_backing() {
    let body = [0x41, 8, 0x41, 1, 0x41, 3, 0xfc, 8, 0, 0, 0x0b];
    let bytes = module(true, &body, b"hello", Some(1));
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    let mut hosts = HostRegistry::new();
    hosts.register_memory("env", "mem", memory.clone()).unwrap();
    let mut vm = Instance::with_hosts(parse_module(&bytes).unwrap(), hosts).unwrap();
    vm.invoke_export("run", &[]).unwrap();
    assert_eq!(memory.read(8, 3).unwrap(), b"ell");
}

#[test]
fn data_drop_empties_segment_and_traps_followup_init() {
    let body = [0xfc, 9, 0, 0x41, 0, 0x41, 0, 0x41, 1, 0xfc, 8, 0, 0, 0x0b];
    let parsed = parse_module(&module(false, &body, b"x", Some(1))).unwrap();
    let mut vm = Instance::new(parsed).unwrap();
    let error = vm.invoke_export("run", &[]).unwrap_err();
    assert!(matches!(
        error,
        RuntimeError::DataSegmentSourceOutOfBounds { .. }
    ));
    assert_eq!(vm.memory().unwrap().bytes()[0], 0);
}

#[test]
fn source_oob_is_atomic() {
    let body = [0x41, 0, 0x41, 4, 0x41, 2, 0xfc, 8, 0, 0, 0x0b];
    let parsed = parse_module(&module(false, &body, b"hello", Some(1))).unwrap();
    let mut vm = Instance::new(parsed).unwrap();
    assert!(matches!(
        vm.invoke_export("run", &[]),
        Err(RuntimeError::DataSegmentSourceOutOfBounds { .. })
    ));
    assert_eq!(&vm.memory().unwrap().bytes()[0..2], &[0, 0]);
}

#[test]
fn destination_oob_is_atomic_for_imported_memory() {
    let body = [
        0x41, 0xff, 0xff, 0x03, 0x41, 0, 0x41, 2, 0xfc, 8, 0, 0, 0x0b,
    ];
    let bytes = module(true, &body, b"xy", Some(1));
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(65_535, &[0x11]).unwrap();
    let mut hosts = HostRegistry::new();
    hosts.register_memory("env", "mem", memory.clone()).unwrap();
    let mut vm = Instance::with_hosts(parse_module(&bytes).unwrap(), hosts).unwrap();
    assert!(matches!(
        vm.invoke_export("run", &[]),
        Err(RuntimeError::MemoryOutOfBounds { .. })
    ));
    assert_eq!(memory.read(65_535, 1).unwrap(), vec![0x11]);
}

#[test]
fn validator_requires_datacount_for_memory_init() {
    let body = [0x41, 0, 0x41, 0, 0x41, 0, 0xfc, 8, 0, 0, 0x0b];
    let parsed = parse_module(&module(false, &body, b"x", None)).unwrap();
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(ValidationError::DataCountRequired { .. }))
    ));
}

#[test]
fn validator_rejects_nonzero_memory_index() {
    let body = [0x41, 0, 0x41, 0, 0x41, 0, 0xfc, 8, 0, 1, 0x0b];
    let parsed = parse_module(&module(false, &body, b"x", Some(1))).unwrap();
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(ValidationError::MemoryIndexOutOfBounds {
            memory_index: 1,
            ..
        }))
    ));
}

#[test]
fn parser_rejects_datacount_mismatch() {
    let body = [0x0b];
    assert!(matches!(
        parse_module(&module(false, &body, b"x", Some(2))),
        Err(ParseError::DataCountMismatch {
            declared: 2,
            actual: 1
        })
    ));
}
