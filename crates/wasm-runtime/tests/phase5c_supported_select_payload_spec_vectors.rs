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

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn module_with_result(result_type: u8, instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x01, result_type]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);

    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn invoke(bytes: &[u8]) -> Value {
    let module = parse_module(bytes).expect("select payload vector must parse");
    validate(&module).expect("select payload vector must validate");
    let mut instance = Instance::new(module).expect("select payload vector must instantiate");
    instance
        .invoke_export("run", &[])
        .expect("select payload vector must execute")
        .expect("select payload vector must return one value")
}

fn i64_select(condition: i32) -> Vec<u8> {
    let mut instructions = vec![
        0x42, 0x02, // i64.const 2
        0x42, 0x01, // i64.const 1
        0x41, // i32.const condition
    ];
    push_i32(&mut instructions, condition);
    instructions.push(0x1b);
    module_with_result(I64, &instructions)
}

fn f32_select(lhs_bits: u32, rhs_bits: u32, condition: i32) -> Vec<u8> {
    let mut instructions = vec![0x43];
    instructions.extend_from_slice(&lhs_bits.to_le_bytes());
    instructions.push(0x43);
    instructions.extend_from_slice(&rhs_bits.to_le_bytes());
    instructions.push(0x41);
    push_i32(&mut instructions, condition);
    instructions.push(0x1b);
    module_with_result(F32, &instructions)
}

fn f64_select(lhs_bits: u64, rhs_bits: u64, condition: i32) -> Vec<u8> {
    let mut instructions = vec![0x44];
    instructions.extend_from_slice(&lhs_bits.to_le_bytes());
    instructions.push(0x44);
    instructions.extend_from_slice(&rhs_bits.to_le_bytes());
    instructions.push(0x41);
    push_i32(&mut instructions, condition);
    instructions.push(0x1b);
    module_with_result(F64, &instructions)
}

#[test]
fn upstream_select_treats_every_nonzero_i32_selector_as_true() {
    // WebAssembly/spec test/core/select.wast @ the pinned revision explicitly
    // checks both -1 and 0xf0f0f0f0 as true selectors.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    for condition in [-1, 0xf0f0_f0f0u32 as i32] {
        assert_eq!(invoke(&i64_select(condition)), Value::I64(2));
    }
}

#[test]
fn upstream_select_preserves_f32_nan_payload_bits() {
    // `nan:0x20304` has this exact binary payload in the f32 encoding.
    let nan_bits = 0x7f82_0304u32;
    let one_bits = 1.0f32.to_bits();
    let two_bits = 2.0f32.to_bits();

    for (lhs_bits, rhs_bits, condition) in [(nan_bits, one_bits, 1), (two_bits, nan_bits, 0)] {
        match invoke(&f32_select(lhs_bits, rhs_bits, condition)) {
            Value::F32(value) => assert_eq!(value.to_bits(), nan_bits),
            other => panic!("expected f32 select result, got {other:?}"),
        }
    }
}

#[test]
fn upstream_select_preserves_f64_nan_payload_bits() {
    // `nan:0x20304` has this exact binary payload in the f64 encoding.
    let nan_bits = 0x7ff0_0000_0002_0304u64;
    let one_bits = 1.0f64.to_bits();
    let two_bits = 2.0f64.to_bits();

    for (lhs_bits, rhs_bits, condition) in [(nan_bits, one_bits, 1), (two_bits, nan_bits, 0)] {
        match invoke(&f64_select(lhs_bits, rhs_bits, condition)) {
            Value::F64(value) => assert_eq!(value.to_bits(), nan_bits),
            other => panic!("expected f64 select result, got {other:?}"),
        }
    }
}
