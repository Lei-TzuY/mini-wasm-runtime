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

fn header() -> Vec<u8> {
    b"\0asm\x01\0\0\0".to_vec()
}

fn memory64_module(result_type: u8, memory_type: &[u8], body: &[u8]) -> Vec<u8> {
    let mut module = header();
    section(&mut module, 1, &[1, 0x60, 0, 1, result_type]);
    section(&mut module, 3, &[1, 0]);

    let mut memory = vec![1];
    memory.extend_from_slice(memory_type);
    section(&mut module, 5, &memory);

    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);

    let mut code = vec![1];
    u32leb(&mut code, body.len() as u32);
    code.extend_from_slice(body);
    section(&mut module, 10, &code);
    module
}

#[test]
fn memory64_size_returns_i64_pages() {
    // memory64, min=1, max=2 (limits flags 0x05).
    let wasm = memory64_module(0x7e, &[0x05, 1, 2], &[0, 0x3f, 0, 0x0b]);
    let module = parse_module(&wasm).expect("memory64 memory type should parse");
    let mut instance = Instance::new(module).expect("memory64 memory.size should validate");
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I64(1))
    );
}

#[test]
fn memory64_grow_consumes_and_returns_i64_pages() {
    // memory64, min=1, max=2. Grow by one page and return the previous size.
    let wasm = memory64_module(0x7e, &[0x05, 1, 2], &[0, 0x42, 1, 0x40, 0, 0x0b]);
    let module = parse_module(&wasm).expect("memory64 memory type should parse");
    let mut instance = Instance::new(module).expect("memory64 memory.grow should validate");
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I64(1))
    );
}

#[test]
fn memory64_scalar_load_consumes_i64_address() {
    // memory64, min=1 and no declared maximum (limits flags 0x04).
    // i64.const 0; i32.load align=2 offset=0
    let wasm = memory64_module(0x7f, &[0x04, 1], &[0, 0x42, 0, 0x28, 2, 0, 0x0b]);
    let module = parse_module(&wasm).expect("memory64 memory type should parse");
    let mut instance = Instance::new(module).expect("memory64 i64-addressed load should validate");
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(0))
    );
}

#[test]
fn memory64_memarg_accepts_u64_static_offset_and_traps_at_runtime_bound() {
    // Memory64 memarg offsets use the 64-bit address width. A static offset
    // above u32::MAX is therefore a valid immediate even though this bounded
    // runtime cannot physically allocate enough memory for the access.
    let static_offset = 1u64 << 32;
    let mut body = vec![0, 0x42, 0, 0x28, 2]; // locals; i64.const 0; i32.load align=2
    u64leb(&mut body, static_offset);
    body.push(0x0b);

    let wasm = memory64_module(0x7f, &[0x04, 1], &body);
    let module = parse_module(&wasm).expect("memory64 memory type should parse");
    let mut instance =
        Instance::new(module).expect("u64 memory64 memarg offset should validate successfully");
    let error = instance
        .invoke_export("run", &[])
        .expect_err("bounded runtime must trap the physically out-of-range access");
    assert!(matches!(
        error,
        RuntimeError::MemoryOutOfBounds { address, width }
            if address == static_offset && width == 4
    ));
}

#[test]
fn memory32_memarg_rejects_static_offset_above_u32_address_width() {
    // The immediate is encoded as u64 for both address widths, but validation
    // must retain the memory32 requirement that OFFSET fits in the i32 address
    // domain instead of silently granting memory64 semantics to memory32.
    let static_offset = 1u64 << 32;
    let mut body = vec![0, 0x41, 0, 0x28, 2]; // locals; i32.const 0; i32.load align=2
    u64leb(&mut body, static_offset);
    body.push(0x0b);

    let wasm = memory64_module(0x7f, &[0x00, 1], &body);
    let module = parse_module(&wasm).expect("memory32 memory type should still parse");
    let error = Instance::new(module)
        .expect_err("memory32 must reject a static offset outside its address width");
    let rendered = error.to_string();
    assert!(matches!(error, RuntimeError::Validation(_)));
    assert!(
        rendered.contains("static offset 4294967296")
            && rendered.contains("address maximum 4294967295"),
        "unexpected memory32 validation error: {rendered}"
    );
}
