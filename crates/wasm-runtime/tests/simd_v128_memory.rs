use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::ValidationError;

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

fn module(memory64: bool, body: &[u8]) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 0, 1, 0x7f]);
    section(&mut module, 3, &[1, 0]);
    if memory64 {
        section(&mut module, 5, &[1, 0x04, 1]);
    } else {
        section(&mut module, 5, &[1, 0x00, 1]);
    }
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);

    let mut code = vec![1];
    u32leb(&mut code, body.len() as u32);
    code.extend_from_slice(body);
    section(&mut module, 10, &code);
    module
}

fn push_v128_const(body: &mut Vec<u8>, bytes: [u8; 16]) {
    body.extend_from_slice(&[0xfd, 0x0c]);
    body.extend_from_slice(&bytes);
}

fn push_v128_load(body: &mut Vec<u8>, offset: u64) {
    body.extend_from_slice(&[0xfd, 0x00, 0x04]);
    u64leb(body, offset);
}

fn push_v128_store(body: &mut Vec<u8>, offset: u64) {
    body.extend_from_slice(&[0xfd, 0x0b, 0x04]);
    u64leb(body, offset);
}

fn round_trip_body(address_opcode: u8) -> Vec<u8> {
    let lanes = [1, 0, 0, 0, 2, 0, 0, 0, 0x11, 0x22, 0x33, 0x44, 4, 0, 0, 0];
    let mut body = vec![0, address_opcode, 8];
    push_v128_const(&mut body, lanes);
    push_v128_store(&mut body, 0);
    body.extend_from_slice(&[address_opcode, 8]);
    push_v128_load(&mut body, 0);
    body.extend_from_slice(&[0xfd, 0x1b, 2, 0x0b]);
    body
}

#[test]
fn v128_store_then_load_round_trips_on_memory32() {
    let wasm = module(false, &round_trip_body(0x41));
    let parsed = parse_module(&wasm).expect("memory32 SIMD memory fixture must parse");
    let mut instance = Instance::new(parsed).expect("memory32 v128 load/store must validate");
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(0x4433_2211))
    );
}

#[test]
fn v128_store_then_load_round_trips_on_memory64() {
    let wasm = module(true, &round_trip_body(0x42));
    let parsed = parse_module(&wasm).expect("memory64 SIMD memory fixture must parse");
    let mut instance = Instance::new(parsed).expect("memory64 v128 load/store must validate");
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(0x4433_2211))
    );
}

#[test]
fn memory64_v128_load_rejects_i32_address_type() {
    let mut body = vec![0, 0x41, 0];
    push_v128_load(&mut body, 0);
    body.extend_from_slice(&[0xfd, 0x1b, 0, 0x0b]);
    let wasm = module(true, &body);
    let parsed = parse_module(&wasm).expect("memory64 SIMD type-confusion fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}

#[test]
fn v128_load_traps_with_full_16_byte_width_at_memory_end() {
    let mut body = vec![0, 0x41, 0];
    push_v128_load(&mut body, 65_530);
    body.extend_from_slice(&[0xfd, 0x1b, 0, 0x0b]);
    let wasm = module(false, &body);
    let parsed = parse_module(&wasm).expect("SIMD OOB fixture must parse");
    let mut instance = Instance::new(parsed).expect("SIMD OOB fixture must validate");
    assert!(matches!(
        instance.invoke_export("run", &[]),
        Err(RuntimeError::MemoryOutOfBounds {
            address: 65_530,
            width: 16
        })
    ));
}

#[test]
fn v128_store_oob_is_atomic() {
    let lanes = [
        0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xab, 0xac, 0xad, 0xae,
        0xaf,
    ];
    let mut body = vec![0, 0x41, 0];
    push_v128_const(&mut body, lanes);
    push_v128_store(&mut body, 65_530);
    body.extend_from_slice(&[0x41, 1, 0x0b]);
    let wasm = module(false, &body);
    let parsed = parse_module(&wasm).expect("SIMD atomic-store fixture must parse");
    let mut instance = Instance::new(parsed).expect("SIMD atomic-store fixture must validate");
    let before = instance.memory().unwrap().bytes()[65_520..].to_vec();
    assert!(matches!(
        instance.invoke_export("run", &[]),
        Err(RuntimeError::MemoryOutOfBounds {
            address: 65_530,
            width: 16
        })
    ));
    assert_eq!(instance.memory().unwrap().bytes()[65_520..], before);
}

#[test]
fn memory64_v128_load_preserves_full_width_static_offset() {
    let offset = 1u64 << 32;
    let mut body = vec![0, 0x42, 0];
    push_v128_load(&mut body, offset);
    body.extend_from_slice(&[0xfd, 0x1b, 0, 0x0b]);
    let wasm = module(true, &body);
    let parsed = parse_module(&wasm).expect("memory64 wide-offset SIMD fixture must parse");
    let mut instance = Instance::new(parsed).expect("memory64 wide SIMD offset must validate");
    assert!(matches!(
        instance.invoke_export("run", &[]),
        Err(RuntimeError::MemoryOutOfBounds { address, width: 16 }) if address == offset
    ));
}

#[test]
fn memory32_v128_load_rejects_static_offset_above_u32_domain() {
    let offset = 1u64 << 32;
    let mut body = vec![0, 0x41, 0];
    push_v128_load(&mut body, offset);
    body.extend_from_slice(&[0xfd, 0x1b, 0, 0x0b]);
    let wasm = module(false, &body);
    let parsed = parse_module(&wasm).expect("memory32 wide-offset SIMD fixture must parse");
    let error = Instance::new(parsed).expect_err("memory32 SIMD offset must stay within u32");
    let rendered = error.to_string();
    assert!(matches!(error, RuntimeError::Validation(_)));
    assert!(
        rendered.contains("static offset 4294967296")
            && rendered.contains("address maximum 4294967295"),
        "unexpected validation error: {rendered}"
    );
}
