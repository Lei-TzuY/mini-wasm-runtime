use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};
use wasm_validator::validate;

const I32: u8 = 0x7f;
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

fn select_br_value_context_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(&mut module, 1, &[0x01, 0x60, 0x01, I32, 0x01, I32]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(
        &mut module,
        7,
        &[
            0x01, 0x0b, b'a', b's', b'-', b'b', b'r', b'-', b'v', b'a', b'l', b'u', b'e',
            0x00, 0x00,
        ],
    );

    let instructions = [
        0x02, I32, // block (result i32)
        0x41, 0x01, // select first candidate: 1
        0x41, 0x02, // select second candidate: 2
        0x20, 0x00, // local.get 0: select condition
        0x1b, // select
        0x0c, 0x00, // br 0 with selected label value
        0x0b, // end block
    ];
    let mut body = vec![0x00];
    body.extend_from_slice(&instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);

    module
}

#[test]
fn pinned_upstream_select_br_value_context_executes() {
    // WebAssembly/spec test/core/select.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let module = parse_module(&select_br_value_context_module())
        .expect("select br-value context vector must parse");
    validate(&module).expect("select br-value context vector must validate");
    let mut instance =
        Instance::new(module).expect("select br-value context vector must instantiate");

    for (condition, expected) in [(0, 2), (1, 1)] {
        assert_eq!(
            instance
                .invoke_export("as-br-value", &[Value::I32(condition)])
                .expect("select br-value context must execute"),
            Some(Value::I32(expected)),
            "unexpected as-br-value result for condition {condition}"
        );
    }
}
