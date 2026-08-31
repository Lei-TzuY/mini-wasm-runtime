use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, RuntimeError};

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

fn push_i32(bytes: &mut Vec<u8>, mut value: i32) {
    loop {
        let byte = (value as u8) & 0x7f;
        let sign_bit_set = byte & 0x40 != 0;
        value >>= 7;
        let done = (value == 0 && !sign_bit_set) || (value == -1 && sign_bit_set);
        bytes.push(if done { byte } else { byte | 0x80 });
        if done {
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

fn push_explicit_active_data(payload: &mut Vec<u8>, offset: i32, bytes: &[u8]) {
    payload.push(0x02); // active data with explicit memory index
    push_u32(payload, 0); // imported memory is memory index 0
    payload.push(0x41); // i32.const
    push_i32(payload, offset);
    payload.push(0x0b); // end const expr
    push_u32(payload, bytes.len() as u32);
    payload.extend_from_slice(bytes);
}

fn imported_memory_module(data_segments: &[(i32, &[u8])]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    let mut imports = vec![0x01];
    push_name(&mut imports, "env");
    push_name(&mut imports, "mem");
    imports.extend([0x02, 0x01, 0x01, 0x02]); // memory, min 1, max 2
    push_section(&mut module, 2, &imports);

    let mut data = Vec::new();
    push_u32(&mut data, data_segments.len() as u32);
    for (offset, bytes) in data_segments {
        push_explicit_active_data(&mut data, *offset, bytes);
    }
    push_section(&mut module, 11, &data);
    module
}

#[test]
fn explicit_mode2_imported_data_preflights_all_segments_before_mutating_shared_memory() {
    let memory = MemoryHandle::new(1, Some(2)).unwrap();
    memory.write(3, b"KEEP").unwrap();

    let module = parse_module(&imported_memory_module(&[
        (3, b"wasm"),
        (65_536, b"x"), // current one-page end: non-empty write is out of bounds
    ]))
    .expect("explicit mode-2 imported data module must parse");
    assert_eq!(module.data[0].memory_index, 0);
    assert_eq!(module.data[1].memory_index, 0);

    let mut hosts = HostRegistry::new();
    hosts.register_memory("env", "mem", memory.clone()).unwrap();

    let error = match Instance::with_hosts(module, hosts) {
        Ok(_) => panic!("later out-of-bounds explicit data segment must fail instantiation"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        RuntimeError::DataSegmentOutOfBounds {
            segment: 1,
            offset: 65_536,
            length: 1,
        }
    ));

    assert_eq!(memory.read(3, 4).unwrap(), b"KEEP");
    assert_eq!(memory.size_pages(), 1);

    let retry = parse_module(&imported_memory_module(&[(3, b"wasm")]))
        .expect("valid explicit mode-2 imported data retry must parse");
    let mut retry_hosts = HostRegistry::new();
    retry_hosts
        .register_memory("env", "mem", memory.clone())
        .unwrap();
    let _instance = Instance::with_hosts(retry, retry_hosts)
        .expect("failed explicit-data preflight must not poison imported memory handle");

    assert_eq!(memory.read(3, 4).unwrap(), b"wasm");
}