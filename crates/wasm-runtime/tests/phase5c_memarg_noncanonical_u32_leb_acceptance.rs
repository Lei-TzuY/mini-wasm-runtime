use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};

const I32: u8 = 0x7f;
const NONCANONICAL_ZERO: [u8; 2] = [0x80, 0x00];
const NONCANONICAL_TWO: [u8; 2] = [0x82, 0x00];
const NONCANONICAL_THREE: [u8; 2] = [0x83, 0x00];

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

fn module_with_body(instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut module, 1, &[0x01, 0x60, 0x01, I32, 0x01, I32]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 5, &[0x01, 0x00, 0x01]);
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

fn invoke(module: &[u8], input: i32) -> i32 {
    let module = parse_module(module).expect("fixture must parse");
    let mut instance = Instance::new(module).expect("noncanonical memargs must validate");
    match instance
        .invoke_export("run", &[Value::I32(input)])
        .expect("noncanonical memargs must execute")
    {
        Some(Value::I32(value)) => value,
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn noncanonical_alignment_and_zero_offset_are_accepted() {
    let mut instructions = vec![
        0x41, 0x08, // i32.const 8
        0x20, 0x00, // local.get 0
        0x36, // i32.store
    ];
    instructions.extend_from_slice(&NONCANONICAL_TWO);
    instructions.extend_from_slice(&NONCANONICAL_ZERO);
    instructions.extend([0x41, 0x08, 0x28]); // i32.const 8; i32.load
    instructions.extend_from_slice(&NONCANONICAL_TWO);
    instructions.extend_from_slice(&NONCANONICAL_ZERO);

    let module = module_with_body(&instructions);
    assert_eq!(invoke(&module, 0x1234_5678), 0x1234_5678);
}

#[test]
fn noncanonical_offsets_preserve_effective_address_semantics() {
    let mut instructions = vec![
        0x41, 0x01, // i32.const 1
        0x20, 0x00, // local.get 0
        0x36, // i32.store
    ];
    instructions.extend_from_slice(&NONCANONICAL_TWO);
    instructions.extend_from_slice(&NONCANONICAL_THREE);
    instructions.extend([0x41, 0x02, 0x28]); // i32.const 2; i32.load
    instructions.extend_from_slice(&NONCANONICAL_TWO);
    instructions.extend_from_slice(&NONCANONICAL_TWO);

    let module = module_with_body(&instructions);
    assert_eq!(invoke(&module, 0x1020_3040), 0x1020_3040);
}
