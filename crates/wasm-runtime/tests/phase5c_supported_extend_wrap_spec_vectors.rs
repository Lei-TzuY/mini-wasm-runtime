use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};
use wasm_validator::validate;

const I32: u8 = 0x7f;
const I64: u8 = 0x7e;
const PINNED_SPEC_COMMIT: &str = "fc209c5ed8afc4dfeb9252024d217da3376c7a6f";

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

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn module(param: u8, result: u8, opcode: u8) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x01, param, 0x01, result]);
    push_section(&mut bytes, 3, &[0x01, 0x00]);
    push_section(&mut bytes, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);

    let body = [0x00, 0x20, 0x00, opcode, 0x0b];
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut bytes, 10, &code);
    bytes
}

fn invoke(param: u8, result: u8, opcode: u8, value: Value) -> Value {
    let module = parse_module(&module(param, result, opcode)).expect("fixture must parse");
    validate(&module).expect("fixture must validate");
    let mut instance = Instance::new(module).expect("fixture must instantiate");
    instance
        .invoke_export("run", &[value])
        .expect("conversion execution must not trap")
        .expect("fixture returns one value")
}

fn assert_pinned_spec() {
    assert_eq!(PINNED_SPEC_COMMIT.len(), 40);
}

#[test]
fn pinned_upstream_i64_extend_i32_s_vectors_sign_extend() {
    assert_pinned_spec();
    for &(input, expected) in &[
        (0, 0),
        (10_000, 10_000),
        (-10_000, -10_000),
        (-1, -1),
        (0x7fff_ffff, 0x0000_0000_7fff_ffff),
        (0x8000_0000u32 as i32, -2_147_483_648),
    ] {
        assert_eq!(
            invoke(I32, I64, 0xac, Value::I32(input)),
            Value::I64(expected)
        );
    }
}

#[test]
fn pinned_upstream_i64_extend_i32_u_vectors_zero_extend() {
    assert_pinned_spec();
    for &(input, expected) in &[
        (0, 0),
        (10_000, 10_000),
        (-10_000, 0x0000_0000_ffff_d8f0),
        (-1, 0x0000_0000_ffff_ffff),
        (0x7fff_ffff, 0x0000_0000_7fff_ffff),
        (0x8000_0000u32 as i32, 0x0000_0000_8000_0000),
    ] {
        assert_eq!(
            invoke(I32, I64, 0xad, Value::I32(input)),
            Value::I64(expected)
        );
    }
}

#[test]
fn pinned_upstream_i32_wrap_i64_vectors_keep_low_32_bits() {
    assert_pinned_spec();
    for &(input, expected) in &[
        (-1, -1),
        (-100_000, -100_000),
        (0x0000_0000_8000_0000, 0x8000_0000u32 as i32),
        (0xffff_ffff_7fff_ffffu64 as i64, 0x7fff_ffff),
        (0xffff_ffff_0000_0000u64 as i64, 0),
        (0xffff_fffe_ffff_ffffu64 as i64, -1),
        (0xffff_ffff_0000_0001u64 as i64, 1),
        (0, 0),
        (1_311_768_467_463_790_320, 0x9abc_def0u32 as i32),
        (0x0000_0000_ffff_ffff, -1),
        (0x0000_0001_0000_0000, 0),
        (0x0000_0001_0000_0001, 1),
    ] {
        assert_eq!(
            invoke(I64, I32, 0xa7, Value::I64(input)),
            Value::I32(expected)
        );
    }
}
