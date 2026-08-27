use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};
use wasm_validator::validate;

const I32: u8 = 0x7f;
const I64: u8 = 0x7e;
const F32: u8 = 0x7d;
const F64: u8 = 0x7c;
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
        .expect("reinterpret execution must not trap")
        .expect("fixture returns one value")
}

fn assert_pinned_spec() {
    assert_eq!(PINNED_SPEC_COMMIT.len(), 40);
}

#[test]
fn pinned_upstream_f32_reinterpret_i32_vectors_preserve_exact_bits() {
    assert_pinned_spec();
    for bits in [
        0x0000_0001u32,
        0x8000_0001,
        123_456_789,
        0x7f80_0000,
        0xff80_0000,
        0x7fa0_0000,
        0xffa0_0000,
    ] {
        let result = invoke(I32, F32, 0xbe, Value::I32(bits as i32)).as_f32();
        assert_eq!(result.to_bits(), bits);
    }
}

#[test]
fn pinned_upstream_f64_reinterpret_i64_vectors_preserve_exact_bits() {
    assert_pinned_spec();
    for bits in [
        0x0000_0000_0000_0001u64,
        0x8000_0000_0000_0001,
        1_234_567_890,
        0x7ff0_0000_0000_0000,
        0xfff0_0000_0000_0000,
        0x7ff4_0000_0000_0000,
        0xfff4_0000_0000_0000,
    ] {
        let result = invoke(I64, F64, 0xbf, Value::I64(bits as i64)).as_f64();
        assert_eq!(result.to_bits(), bits);
    }
}

#[test]
fn pinned_upstream_i32_reinterpret_f32_vectors_preserve_exact_bits() {
    assert_pinned_spec();
    for bits in [
        0x0000_0001u32,
        0x8000_0001,
        0x3f80_0000,
        1_078_530_010,
        0x7f7f_ffff,
        0x7f80_0000,
        0xff80_0000,
        0x7fc0_0000,
        0x7fa0_0000,
        0xffa0_0000,
    ] {
        assert_eq!(
            invoke(F32, I32, 0xbc, Value::F32(f32::from_bits(bits))),
            Value::I32(bits as i32)
        );
    }
}

#[test]
fn pinned_upstream_i64_reinterpret_f64_vectors_preserve_exact_bits() {
    assert_pinned_spec();
    for bits in [
        0x0000_0000_0000_0001u64,
        0x8000_0000_0000_0001,
        0x3ff0_0000_0000_0000,
        4_614_256_656_552_045_841,
        0x7fef_ffff_ffff_ffff,
        0x7ff0_0000_0000_0000,
        0xfff0_0000_0000_0000,
        0x7ff8_0000_0000_0000,
        0x7ff4_0000_0000_0000,
        0xfff4_0000_0000_0000,
    ] {
        assert_eq!(
            invoke(F64, I64, 0xbd, Value::F64(f64::from_bits(bits))),
            Value::I64(bits as i64)
        );
    }
}
