use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};

const I32: u8 = 0x7f;
const NONCANONICAL_TYPE_ZERO: [u8; 2] = [0x80, 0x00];

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

fn module_with_body(body: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    // type 0: () -> i32; the structured-control opener reuses this type index.
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x01, I32]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);

    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(body);
    push_section(&mut module, 10, &code);
    module
}

fn invoke(body: &[u8]) -> Value {
    let module = parse_module(&module_with_body(body)).expect("noncanonical blocktype must parse");
    let mut instance = Instance::new(module).expect("noncanonical blocktype must validate");
    instance
        .invoke_export("run", &[])
        .expect("noncanonical blocktype must execute")
        .expect("fixture returns i32")
}

#[test]
fn block_accepts_noncanonical_signed_33_type_index() {
    let mut body = vec![0x00, 0x02]; // no locals; block
    body.extend_from_slice(&NONCANONICAL_TYPE_ZERO);
    body.extend([0x41, 0x2a, 0x0b, 0x0b]); // i32.const 42; end block; end function

    assert_eq!(invoke(&body), Value::I32(42));
}

#[test]
fn loop_accepts_noncanonical_signed_33_type_index() {
    let mut body = vec![0x00, 0x03]; // no locals; loop
    body.extend_from_slice(&NONCANONICAL_TYPE_ZERO);
    body.extend([0x41, 0x2a, 0x0b, 0x0b]); // i32.const 42; end loop; end function

    assert_eq!(invoke(&body), Value::I32(42));
}

#[test]
fn if_accepts_noncanonical_signed_33_type_index() {
    let mut body = vec![0x00, 0x41, 0x01, 0x04]; // no locals; i32.const 1; if
    body.extend_from_slice(&NONCANONICAL_TYPE_ZERO);
    body.extend([
        0x41, 0x2a, // then: i32.const 42
        0x05, // else
        0x41, 0x07, // else: i32.const 7
        0x0b, // end if
        0x0b, // end function
    ]);

    assert_eq!(invoke(&body), Value::I32(42));
}
