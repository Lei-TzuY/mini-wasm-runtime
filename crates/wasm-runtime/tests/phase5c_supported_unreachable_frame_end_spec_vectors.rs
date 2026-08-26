use wasm_parser::parse_module;
use wasm_runtime::Instance;

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

fn build_i32_result_module(instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x01, 0x7f]);
    push_section(&mut module, 3, &[0x01, 0x00]);

    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn assert_instantiates(instructions: &[u8]) {
    let module = parse_module(&build_i32_result_module(instructions))
        .expect("fixture must remain structurally parseable");
    Instance::new(module).expect("valid unreachable frame-end polymorphism must be accepted");
}

#[test]
fn result_block_accepts_polymorphic_end_after_return() {
    assert_instantiates(&[
        0x02, 0x7f, // block (result i32)
        0x41, 0x07, // i32.const 7: function return value
        0x0f, // return makes the current block unreachable
        0x0b, // block end has no concrete result value
    ]);
}

#[test]
fn result_loop_accepts_polymorphic_end_after_return() {
    assert_instantiates(&[
        0x03, 0x7f, // loop (result i32)
        0x41, 0x09, // i32.const 9: function return value
        0x0f, // return makes the current loop unreachable
        0x0b, // loop end has no concrete result value
    ]);
}

#[test]
fn result_if_then_arm_can_end_polymorphically_before_reachable_else() {
    assert_instantiates(&[
        0x41, 0x01, // i32.const 1: condition
        0x04, 0x7f, // if (result i32)
        0x41, 0x05, // then: function return value
        0x0f, // return makes only the then arm unreachable
        0x05, // else must restart a reachable validation context
        0x41, 0x06, // else: concrete i32 result
        0x0b,
    ]);
}
