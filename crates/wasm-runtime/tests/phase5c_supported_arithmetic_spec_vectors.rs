use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};
use wasm_validator::validate;

const I64: u8 = 0x7e;
const F32: u8 = 0x7d;
const F64: u8 = 0x7c;
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

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn binary_module(value_type: u8, opcode: u8) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(
        &mut module,
        1,
        &[0x01, 0x60, 0x02, value_type, value_type, 0x01, value_type],
    );
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);

    let body = [0x00, 0x20, 0x00, 0x20, 0x01, opcode, 0x0b];
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn invoke(opcode: u8, value_type: u8, args: &[Value]) -> Value {
    let module = parse_module(&binary_module(value_type, opcode))
        .expect("pinned arithmetic vector must parse");
    validate(&module).expect("pinned arithmetic vector must validate");
    let mut instance = Instance::new(module).expect("pinned arithmetic vector must instantiate");
    instance
        .invoke_export("run", args)
        .expect("pinned arithmetic vector must execute")
        .expect("pinned arithmetic vector must return one value")
}

fn invoke_i64(opcode: u8, lhs: i64, rhs: i64) -> i64 {
    match invoke(opcode, I64, &[Value::I64(lhs), Value::I64(rhs)]) {
        Value::I64(value) => value,
        other => panic!("expected i64 result, got {other:?}"),
    }
}

fn invoke_f32_bits(opcode: u8, lhs_bits: u32, rhs_bits: u32) -> u32 {
    match invoke(
        opcode,
        F32,
        &[
            Value::F32(f32::from_bits(lhs_bits)),
            Value::F32(f32::from_bits(rhs_bits)),
        ],
    ) {
        Value::F32(value) => value.to_bits(),
        other => panic!("expected f32 result, got {other:?}"),
    }
}

fn invoke_f64_bits(opcode: u8, lhs_bits: u64, rhs_bits: u64) -> u64 {
    match invoke(
        opcode,
        F64,
        &[
            Value::F64(f64::from_bits(lhs_bits)),
            Value::F64(f64::from_bits(rhs_bits)),
        ],
    ) {
        Value::F64(value) => value.to_bits(),
        other => panic!("expected f64 result, got {other:?}"),
    }
}

#[test]
fn pinned_upstream_i64_add_sub_mul_wrap_vectors_match_spec() {
    // WebAssembly/spec test/core/i64.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    for (opcode, lhs, rhs, expected) in [
        (0x7c, i64::MAX, 1, i64::MIN),
        (0x7c, i64::MIN, -1, i64::MAX),
        (0x7d, i64::MAX, -1, i64::MIN),
        (0x7d, i64::MIN, 1, i64::MAX),
        (0x7e, 0x1000_0000_0000_0000, 4096, 0),
        (0x7e, i64::MIN, -1, i64::MIN),
        (
            0x7e,
            0x0123_4567_89ab_cdef,
            0xfedc_ba98_7654_3210u64 as i64,
            0x2236_d88f_e561_8cf0,
        ),
    ] {
        assert_eq!(invoke_i64(opcode, lhs, rhs), expected);
    }
}

#[test]
fn pinned_upstream_f32_add_sub_mul_signed_zero_and_subnormal_vectors_match_spec() {
    // WebAssembly/spec test/core/f32.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    for (opcode, lhs_bits, rhs_bits, expected_bits) in [
        (0x92, 0x8000_0000, 0x8000_0000, 0x8000_0000), // -0 + -0 = -0
        (0x92, 0x0000_0001, 0x0000_0001, 0x0000_0002), // min subnormal + itself
        (0x93, 0x8000_0000, 0x0000_0000, 0x8000_0000), // -0 - +0 = -0
        (0x93, 0x0000_0000, 0x0000_0001, 0x8000_0001), // +0 - min subnormal
        (0x94, 0x8000_0000, 0x3f80_0000, 0x8000_0000), // -0 * +1 = -0
        (0x94, 0x8000_0000, 0x8000_0000, 0x0000_0000), // -0 * -0 = +0
    ] {
        assert_eq!(
            invoke_f32_bits(opcode, lhs_bits, rhs_bits),
            expected_bits,
            "unexpected f32 result for opcode 0x{opcode:02x}"
        );
    }
}

#[test]
fn pinned_upstream_f64_add_sub_mul_signed_zero_and_subnormal_vectors_match_spec() {
    // WebAssembly/spec test/core/f64.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    for (opcode, lhs_bits, rhs_bits, expected_bits) in [
        (
            0xa0,
            0x8000_0000_0000_0000,
            0x8000_0000_0000_0000,
            0x8000_0000_0000_0000,
        ), // -0 + -0 = -0
        (
            0xa0,
            0x0000_0000_0000_0001,
            0x0000_0000_0000_0001,
            0x0000_0000_0000_0002,
        ), // min subnormal + itself
        (
            0xa1,
            0x8000_0000_0000_0000,
            0x0000_0000_0000_0000,
            0x8000_0000_0000_0000,
        ), // -0 - +0 = -0
        (
            0xa1,
            0x0000_0000_0000_0000,
            0x0000_0000_0000_0001,
            0x8000_0000_0000_0001,
        ), // +0 - min subnormal
        (
            0xa2,
            0x8000_0000_0000_0000,
            0x3ff0_0000_0000_0000,
            0x8000_0000_0000_0000,
        ), // -0 * +1 = -0
        (
            0xa2,
            0x8000_0000_0000_0000,
            0x8000_0000_0000_0000,
            0x0000_0000_0000_0000,
        ), // -0 * -0 = +0
    ] {
        assert_eq!(
            invoke_f64_bits(opcode, lhs_bits, rhs_bits),
            expected_bits,
            "unexpected f64 result for opcode 0x{opcode:02x}"
        );
    }
}
