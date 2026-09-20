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

fn module(memory64: bool, body: &[u8], data: &[u8]) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 0, 1, 0x7b]);
    section(&mut module, 3, &[1, 0]);
    section(
        &mut module,
        5,
        if memory64 {
            &[1, 0x04, 1]
        } else {
            &[1, 0x00, 1]
        },
    );
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);

    let mut code = vec![1];
    u32leb(&mut code, body.len() as u32);
    code.extend_from_slice(body);
    section(&mut module, 10, &code);

    if !data.is_empty() {
        assert!(!memory64, "focused data helper is memory32-only");
        let mut payload = vec![1, 0, 0x41, 0, 0x0b];
        u32leb(&mut payload, data.len() as u32);
        payload.extend_from_slice(data);
        section(&mut module, 11, &payload);
    }
    module
}

fn push_simd_load(body: &mut Vec<u8>, subopcode: u8, align: u8, offset: u64) {
    body.extend_from_slice(&[0xfd, subopcode, align]);
    u64leb(body, offset);
}

fn execute_memory32(subopcode: u8, align: u8, offset: u64, data: &[u8]) -> [u8; 16] {
    let mut body = vec![0, 0x41, 0];
    push_simd_load(&mut body, subopcode, align, offset);
    body.push(0x0b);
    let parsed = parse_module(&module(false, &body, data)).expect("fixture parses");
    let mut instance = Instance::new(parsed).expect("fixture validates");
    match instance
        .invoke_export_values("run", &[])
        .expect("fixture executes")
        .as_slice()
    {
        [Value::V128(value)] => **value,
        other => panic!("unexpected SIMD load result: {other:?}"),
    }
}

#[test]
fn widening_loads_extend_each_lane_with_exact_signedness() {
    let bytes = [
        0x80, 0x7f, 0xff, 0x01, 0x00, 0xaa, 0x55, 0xfe, 0x00, 0x80, 0xff, 0x7f, 0xff, 0xff, 0x34,
        0x12, 0x00, 0x00, 0x00, 0x80, 0xff, 0xff, 0xff, 0x7f,
    ];

    assert_eq!(
        execute_memory32(1, 3, 0, &bytes),
        [
            0x80, 0xff, 0x7f, 0x00, 0xff, 0xff, 0x01, 0x00, 0x00, 0x00, 0xaa, 0xff, 0x55, 0x00,
            0xfe, 0xff
        ]
    );
    assert_eq!(
        execute_memory32(2, 3, 0, &bytes),
        [
            0x80, 0x00, 0x7f, 0x00, 0xff, 0x00, 0x01, 0x00, 0x00, 0x00, 0xaa, 0x00, 0x55, 0x00,
            0xfe, 0x00
        ]
    );
    assert_eq!(
        execute_memory32(3, 3, 8, &bytes),
        [
            0x00, 0x80, 0xff, 0xff, 0xff, 0x7f, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0x34, 0x12,
            0x00, 0x00
        ]
    );
    assert_eq!(
        execute_memory32(4, 3, 8, &bytes),
        [
            0x00, 0x80, 0x00, 0x00, 0xff, 0x7f, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x34, 0x12,
            0x00, 0x00
        ]
    );
    assert_eq!(
        execute_memory32(5, 3, 16, &bytes),
        [
            0x00, 0x00, 0x00, 0x80, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f, 0x00, 0x00,
            0x00, 0x00
        ]
    );
    assert_eq!(
        execute_memory32(6, 3, 16, &bytes),
        [
            0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0x7f, 0x00, 0x00,
            0x00, 0x00
        ]
    );
}

#[test]
fn splat_loads_repeat_the_exact_loaded_element() {
    let bytes = [
        0x80, 0x34, 0x12, 0x78, 0x56, 0x34, 0x12, 0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01,
    ];
    assert_eq!(execute_memory32(7, 0, 0, &bytes), [0x80; 16]);
    assert_eq!(
        execute_memory32(8, 1, 1, &bytes),
        [
            0x34, 0x12, 0x34, 0x12, 0x34, 0x12, 0x34, 0x12, 0x34, 0x12, 0x34, 0x12, 0x34, 0x12,
            0x34, 0x12
        ]
    );
    assert_eq!(
        execute_memory32(9, 2, 3, &bytes),
        [
            0x78, 0x56, 0x34, 0x12, 0x78, 0x56, 0x34, 0x12, 0x78, 0x56, 0x34, 0x12, 0x78, 0x56,
            0x34, 0x12
        ]
    );
    assert_eq!(
        execute_memory32(10, 3, 7, &bytes),
        [
            0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, 0x08, 0x07, 0x06, 0x05, 0x04, 0x03,
            0x02, 0x01
        ]
    );
}

#[test]
fn widening_and_splat_loads_enforce_natural_alignment() {
    for (subopcode, invalid_align) in [(1, 4), (6, 4), (7, 1), (8, 2), (9, 3), (10, 4)] {
        let mut body = vec![0, 0x41, 0];
        push_simd_load(&mut body, subopcode, invalid_align, 0);
        body.push(0x0b);
        let parsed = parse_module(&module(false, &body, &[])).expect("fixture parses");
        assert!(
            matches!(
                Instance::new(parsed),
                Err(RuntimeError::Validation(
                    ValidationError::InvalidMemoryAlignment { .. }
                ))
            ),
            "subopcode {subopcode} must reject alignment 2^{invalid_align}"
        );
    }
}

#[test]
fn memory64_widening_load_requires_i64_address_and_executes() {
    let mut wrong = vec![0, 0x41, 0];
    push_simd_load(&mut wrong, 1, 3, 0);
    wrong.push(0x0b);
    let parsed = parse_module(&module(true, &wrong, &[])).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));

    let mut correct = vec![0, 0x42, 0];
    push_simd_load(&mut correct, 1, 3, 0);
    correct.push(0x0b);
    let parsed = parse_module(&module(true, &correct, &[])).expect("fixture parses");
    let mut instance = Instance::new(parsed).expect("memory64 widening load validates");
    assert!(matches!(
        instance.invoke_export_values("run", &[]).unwrap().as_slice(),
        [Value::V128(value)] if value.iter().all(|byte| *byte == 0)
    ));
}

#[test]
fn load_extend_uses_full_eight_byte_bounds_preflight() {
    let mut body = vec![0, 0x41, 0];
    push_simd_load(&mut body, 1, 3, 65_529);
    body.push(0x0b);
    let parsed = parse_module(&module(false, &body, &[])).expect("fixture parses");
    let mut instance = Instance::new(parsed).expect("fixture validates");
    assert!(matches!(
        instance.invoke_export_values("run", &[]),
        Err(RuntimeError::MemoryOutOfBounds {
            address: 65_529,
            width: 8
        })
    ));
}

#[test]
fn memory64_load_extend_preserves_full_width_static_offset() {
    let offset = 1u64 << 32;
    let mut body = vec![0, 0x42, 0];
    push_simd_load(&mut body, 1, 3, offset);
    body.push(0x0b);
    let parsed = parse_module(&module(true, &body, &[])).expect("fixture parses");
    let mut instance = Instance::new(parsed).expect("wide memory64 offset validates");
    assert!(matches!(
        instance.invoke_export_values("run", &[]),
        Err(RuntimeError::MemoryOutOfBounds { address, width: 8 }) if address == offset
    ));
}

#[test]
fn memory32_load_extend_rejects_static_offset_above_u32_domain() {
    let offset = 1u64 << 32;
    let mut body = vec![0, 0x41, 0];
    push_simd_load(&mut body, 1, 3, offset);
    body.push(0x0b);
    let parsed = parse_module(&module(false, &body, &[])).expect("fixture parses");
    let error = Instance::new(parsed).expect_err("memory32 offset must stay within u32");
    let rendered = error.to_string();
    assert!(matches!(error, RuntimeError::Validation(_)));
    assert!(
        rendered.contains("static offset 4294967296")
            && rendered.contains("address maximum 4294967295"),
        "unexpected validation error: {rendered}"
    );
}
