use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, MemoryHandle, RuntimeError, TableHandle};

const UPSTREAM_SPEC_COMMIT: &str = "fc209c5ed8afc4dfeb9252024d217da3376c7a6f";

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

fn push_limits(payload: &mut Vec<u8>, minimum: u32, maximum: Option<u32>) {
    match maximum {
        Some(maximum) => {
            payload.push(0x01);
            push_u32(payload, minimum);
            push_u32(payload, maximum);
        }
        None => {
            payload.push(0x00);
            push_u32(payload, minimum);
        }
    }
}

fn push_body(payload: &mut Vec<u8>, instructions: &[u8]) {
    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    push_u32(payload, body.len() as u32);
    payload.extend_from_slice(&body);
}

fn imported_memory_data_module(
    minimum: u32,
    maximum: Option<u32>,
    offset: i32,
    bytes: &[u8],
) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    let mut imports = vec![0x01];
    push_name(&mut imports, "env");
    push_name(&mut imports, "mem");
    imports.push(0x02);
    push_limits(&mut imports, minimum, maximum);
    push_section(&mut module, 2, &imports);

    let mut data = vec![0x01, 0x00, 0x41];
    push_i32(&mut data, offset);
    data.push(0x0b);
    push_u32(&mut data, bytes.len() as u32);
    data.extend_from_slice(bytes);
    push_section(&mut module, 11, &data);
    module
}

fn imported_table_element_module(minimum: u32, maximum: Option<u32>, offset: i32) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x00]);

    let mut imports = vec![0x01];
    push_name(&mut imports, "env");
    push_name(&mut imports, "tab");
    imports.extend([0x01, 0x70]);
    push_limits(&mut imports, minimum, maximum);
    push_section(&mut module, 2, &imports);

    push_section(&mut module, 3, &[0x01, 0x00]);

    let mut elements = vec![0x01, 0x00, 0x41];
    push_i32(&mut elements, offset);
    elements.extend([0x0b, 0x01, 0x00]);
    push_section(&mut module, 9, &elements);

    let mut code = vec![0x01];
    push_body(&mut code, &[]);
    push_section(&mut module, 10, &code);
    module
}

#[test]
fn imported_active_data_failure_uses_current_size_and_preserves_host_backing() {
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    for (offset, expected_offset) in [
        (0x1_0000, 0x1_0000u64),
        (-1, u64::from(u32::MAX)),
        (-100, u64::from(u32::MAX - 99)),
    ] {
        let memory = MemoryHandle::new(1, Some(2)).unwrap();
        memory.write(0, b"K").unwrap();
        let module = parse_module(&imported_memory_data_module(1, Some(2), offset, b"a"))
            .expect("imported data boundary vector must parse");
        let mut hosts = HostRegistry::new();
        hosts.register_memory("env", "mem", memory.clone()).unwrap();

        let error = match Instance::with_hosts(module, hosts) {
            Ok(_) => panic!("imported data at offset {offset} must fail instantiation"),
            Err(error) => error,
        };
        assert!(
            matches!(
                error,
                RuntimeError::DataSegmentOutOfBounds {
                    segment: 0,
                    offset,
                    length: 1,
                } if offset == expected_offset
            ),
            "unexpected imported-data error at offset {offset}: {error:?}"
        );
        assert_eq!(memory.read(0, 1).unwrap(), b"K");
        assert_eq!(memory.size_pages(), 1);
    }
}

#[test]
fn imported_active_element_failure_preserves_slots_and_does_not_poison_binding() {
    for (offset, expected_offset) in [
        (10, 10u64),
        (-1, u64::from(u32::MAX)),
        (-10, u64::from(u32::MAX - 9)),
    ] {
        let table = TableHandle::new(10, Some(20)).unwrap();
        let module = parse_module(&imported_table_element_module(10, Some(20), offset))
            .expect("imported element boundary vector must parse");
        let mut hosts = HostRegistry::new();
        hosts.register_table("env", "tab", table.clone()).unwrap();

        let error = match Instance::with_hosts(module, hosts) {
            Ok(_) => panic!("imported element at offset {offset} must fail instantiation"),
            Err(error) => error,
        };
        assert!(
            matches!(
                error,
                RuntimeError::ElementSegmentOutOfBounds {
                    segment: 0,
                    offset,
                    length: 1,
                } if offset == expected_offset
            ),
            "unexpected imported-element error at offset {offset}: {error:?}"
        );
        for index in 0..10 {
            assert!(table.get(index).unwrap().is_none());
        }

        let valid = parse_module(&imported_table_element_module(10, Some(20), 9))
            .expect("valid imported element vector must parse");
        let mut retry_hosts = HostRegistry::new();
        retry_hosts
            .register_table("env", "tab", table.clone())
            .unwrap();
        let _instance = Instance::with_hosts(valid, retry_hosts)
            .expect("failed preflight must not leave imported table bound");
        assert!(table.get(9).unwrap().is_some());
    }
}
