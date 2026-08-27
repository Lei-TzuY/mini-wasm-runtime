use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};
use wasm_validator::validate;

const I32: u8 = 0x7f;
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

fn comparison_module(value_type: u8, opcode: u8) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(
        &mut module,
        1,
        &[
            0x01, 0x60, 0x02, value_type, value_type, 0x01, I32,
        ],
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

fn invoke(opcode: u8, value_type: u8, args: &[Value]) -> i32 {
    let module = parse_module(&comparison_module(value_type, opcode))
        .expect("pinned float comparison vector must parse");
    validate(&module).expect("pinned float comparison vector must validate");
    let mut instance =
        Instance::new(module).expect("pinned float comparison vector must instantiate");
    match instance
        .invoke_export("run", args)
        .expect("pinned float comparison vector must execute")
        .expect("pinned float comparison vector must return one value")
    {
        Value::I32(value) => value,
        other => panic!("expected i32 comparison result, got {other:?}"),
    }
}

fn assert_f32(opcode: u8, lhs_bits: u32, rhs_bits: u32, expected: i32) {
    assert_eq!(
        invoke(
            opcode,
            F32,
            &[
                Value::F32(f32::from_bits(lhs_bits)),
                Value::F32(f32::from_bits(rhs_bits)),
            ],
        ),
        expected,
        "unexpected f32 comparison for opcode 0x{opcode:02x}, lhs=0x{lhs_bits:08x}, rhs=0x{rhs_bits:08x}"
    );
}

fn assert_f64(opcode: u8, lhs_bits: u64, rhs_bits: u64, expected: i32) {
    assert_eq!(
        invoke(
            opcode,
            F64,
            &[
                Value::F64(f64::from_bits(lhs_bits)),
                Value::F64(f64::from_bits(rhs_bits)),
            ],
        ),
        expected,
        "unexpected f64 comparison for opcode 0x{opcode:02x}, lhs=0x{lhs_bits:016x}, rhs=0x{rhs_bits:016x}"
    );
}

#[test]
fn pinned_upstream_f32_nan_comparisons_are_unordered() {
    // WebAssembly/spec test/core/f32.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let nan = 0x7fc0_1234;
    let one = 0x3f80_0000;
    for (opcode, expected) in [
        (0x5b, 0), // eq
        (0x5c, 1), // ne
        (0x5d, 0), // lt
        (0x5e, 0), // gt
        (0x5f, 0), // le
        (0x60, 0), // ge
    ] {
        assert_f32(opcode, nan, one, expected);
        assert_f32(opcode, one, nan, expected);
        assert_f32(opcode, nan, nan, expected);
    }
}

#[test]
fn pinned_upstream_f64_nan_comparisons_are_unordered() {
    // WebAssembly/spec test/core/f64.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let nan = 0x7ff8_0000_0000_1234;
    let one = 0x3ff0_0000_0000_0000;
    for (opcode, expected) in [
        (0x61, 0), // eq
        (0x62, 1), // ne
        (0x63, 0), // lt
        (0x64, 0), // gt
        (0x65, 0), // le
        (0x66, 0), // ge
    ] {
        assert_f64(opcode, nan, one, expected);
        assert_f64(opcode, one, nan, expected);
        assert_f64(opcode, nan, nan, expected);
    }
}

#[test]
fn pinned_upstream_f32_signed_zero_comparisons_match_numeric_equality() {
    let positive_zero = 0x0000_0000;
    let negative_zero = 0x8000_0000;
    for (opcode, expected) in [
        (0x5b, 1), // eq
        (0x5c, 0), // ne
        (0x5d, 0), // lt
        (0x5e, 0), // gt
        (0x5f, 1), // le
        (0x60, 1), // ge
    ] {
        assert_f32(opcode, positive_zero, negative_zero, expected);
        assert_f32(opcode, negative_zero, positive_zero, expected);
    }
}

#[test]
fn pinned_upstream_f64_signed_zero_comparisons_match_numeric_equality() {
    let positive_zero = 0x0000_0000_0000_0000;
    let negative_zero = 0x8000_0000_0000_0000;
    for (opcode, expected) in [
        (0x61, 1), // eq
        (0x62, 0), // ne
        (0x63, 0), // lt
        (0x64, 0), // gt
        (0x65, 1), // le
        (0x66, 1), // ge
    ] {
        assert_f64(opcode, positive_zero, negative_zero, expected);
        assert_f64(opcode, negative_zero, positive_zero, expected);
    }
}
