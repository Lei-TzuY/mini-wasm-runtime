use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};

fn u32leb(out: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn u64leb(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    u32leb(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn module(memories: &[u8], body: &[u8], passive_data: Option<&[u8]>) -> Vec<u8> {
    let mut wasm = b"\0asm\x01\0\0\0".to_vec();
    section(&mut wasm, 1, &[1, 0x60, 0, 1, 0x7f]);
    section(&mut wasm, 3, &[1, 0]);

    let mut memory_section = vec![(memories.len() / 2) as u8];
    memory_section.extend_from_slice(memories);
    section(&mut wasm, 5, &memory_section);
    section(&mut wasm, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);

    if passive_data.is_some() {
        section(&mut wasm, 12, &[1]);
    }

    let mut function_body = vec![0];
    function_body.extend_from_slice(body);
    function_body.push(0x0b);
    let mut code = vec![1];
    u32leb(&mut code, function_body.len() as u32);
    code.extend_from_slice(&function_body);
    section(&mut wasm, 10, &code);

    if let Some(data) = passive_data {
        let mut data_section = vec![1, 1];
        u32leb(&mut data_section, data.len() as u32);
        data_section.extend_from_slice(data);
        section(&mut wasm, 11, &data_section);
    }
    wasm
}

fn one_memory64(body: &[u8], passive_data: Option<&[u8]>) -> Vec<u8> {
    // flags=0x04 (memory64), min=1 page.
    module(&[0x04, 1], body, passive_data)
}

#[test]
fn memory64_fill_uses_i64_destination_and_length() {
    let body = [
        0x42, 3, // i64.const destination
        0x41, 0x2a, // i32.const fill value
        0x42, 4, // i64.const length
        0xfc, 0x0b, 0, // memory.fill 0
        0x42, 6, 0x2d, 0, 0, // i32.load8_u memory 0
    ];
    let mut instance = Instance::new(parse_module(&one_memory64(&body, None)).unwrap())
        .expect("memory64 memory.fill should validate with i64 destination/length");
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(0x2a))
    );
}

#[test]
fn memory64_init_uses_i64_destination_but_i32_segment_offsets() {
    let body = [
        0x42, 4, // i64.const destination
        0x41, 1, // i32.const passive-data source offset
        0x41, 3, // i32.const length
        0xfc, 8, 0, 0, // memory.init data=0 memory=0
        0x42, 6, 0x2d, 0, 0, // i32.load8_u memory 0
    ];
    let mut instance = Instance::new(parse_module(&one_memory64(&body, Some(b"hello"))).unwrap())
        .expect("memory64 memory.init should keep data source/length i32");
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(b'l' as i32))
    );
}

#[test]
fn memory64_copy_uses_i64_destination_source_and_length() {
    let body = [
        0x42, 1, 0x41, 0xda, 0x00, 0x42, 1, 0xfc, 0x0b, 0, // fill byte 1 with 'Z'
        0x42, 5, 0x42, 1, 0x42, 1, 0xfc, 0x0a, 0, 0, // copy 1 -> 5
        0x42, 5, 0x2d, 0, 0, // load byte 5
    ];
    let mut instance = Instance::new(parse_module(&one_memory64(&body, None)).unwrap())
        .expect("memory64 memory.copy should validate with three i64 operands");
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(0x5a))
    );
}

#[test]
fn mixed_memory_copy_uses_per_memory_addresses_and_i32_length() {
    // memory 0 = memory64, memory 1 = memory32.
    let memories = [0x04, 1, 0x00, 1];
    let body = [
        0x41, 0, // destination in memory32 => i32
        0x42, 0, // source in memory64 => i64
        0x41, 0, // mixed-width length => i32
        0xfc, 0x0a, 1, 0, // memory.copy destination=1 source=0
        0x41, 7, // result
    ];
    let mut instance = Instance::new(parse_module(&module(&memories, &body, None)).unwrap())
        .expect("mixed memory64->memory32 copy should validate with i32 length");
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(7))
    );
}

#[test]
fn mixed_memory_copy_rejects_i64_length() {
    let memories = [0x04, 1, 0x00, 1];
    let body = [
        0x41, 0, // destination memory32
        0x42, 0, // source memory64
        0x42, 0, // invalid: mixed-width length must remain i32
        0xfc, 0x0a, 1, 0, 0x41, 0,
    ];
    let parsed = parse_module(&module(&memories, &body, None)).unwrap();
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(_))
    ));
}

#[test]
fn memory64_bulk_address_above_u32_is_not_truncated() {
    let mut body = vec![0x42];
    u64leb(&mut body, 1u64 << 32);
    body.extend([
        0x41, 0x11, // fill value
        0x42, 1, // i64 length
        0xfc, 0x0b, 0, // memory.fill 0
        0x41, 0, // unreachable result after trap
    ]);
    let mut instance = Instance::new(parse_module(&one_memory64(&body, None)).unwrap())
        .expect("full-width memory64 bulk address should validate");
    assert!(matches!(
        instance.invoke_export("run", &[]),
        Err(RuntimeError::MemoryOutOfBounds { address, .. }) if address >= (1u64 << 32)
    ));
    assert_eq!(instance.memory().unwrap().bytes()[0], 0);
}
