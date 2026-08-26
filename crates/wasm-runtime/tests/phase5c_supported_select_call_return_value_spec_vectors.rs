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

fn push_name(bytes: &mut Vec<u8>, name: &str) {
    push_u32(bytes, name.len() as u32);
    bytes.extend_from_slice(name.as_bytes());
}

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn push_export(payload: &mut Vec<u8>, name: &str, function_index: u32) {
    push_name(payload, name);
    payload.push(0x00);
    push_u32(payload, function_index);
}

fn push_body(payload: &mut Vec<u8>, instructions: &[u8]) {
    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    push_u32(payload, body.len() as u32);
    payload.extend_from_slice(&body);
}

fn select_call_return_value_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(&mut module, 1, &[0x01, 0x60, 0x01, I32, 0x01, I32]);
    push_section(&mut module, 3, &[0x03, 0x00, 0x00, 0x00]);

    let exports = [("as-call-value", 1), ("as-return-value", 2)];
    let mut export_section = Vec::new();
    push_u32(&mut export_section, exports.len() as u32);
    for (name, function_index) in exports {
        push_export(&mut export_section, name, function_index);
    }
    push_section(&mut module, 7, &export_section);

    let mut code = vec![0x03];
    push_body(
        &mut code,
        &[
            0x20, 0x00, // identity: local.get 0
        ],
    );
    push_body(
        &mut code,
        &[
            0x41, 0x01, // select first candidate: 1
            0x41, 0x02, // select second candidate: 2
            0x20, 0x00, // select condition
            0x1b, // select
            0x10, 0x00, // call identity
        ],
    );
    push_body(
        &mut code,
        &[
            0x41, 0x01, // select first candidate: 1
            0x41, 0x02, // select second candidate: 2
            0x20, 0x00, // select condition
            0x1b, // select
            0x0f, // return selected value
        ],
    );
    push_section(&mut module, 10, &code);

    module
}

#[test]
fn pinned_upstream_select_call_and_return_value_contexts_execute_both_directions() {
    // WebAssembly/spec test/core/select.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let module = parse_module(&select_call_return_value_module())
        .expect("select call/return value context vector must parse");
    validate(&module).expect("select call/return value context vector must validate");
    let mut instance =
        Instance::new(module).expect("select call/return value context vector must instantiate");

    for (name, condition, expected) in [
        ("as-call-value", 0, 2),
        ("as-call-value", 1, 1),
        ("as-return-value", 0, 2),
        ("as-return-value", 1, 1),
    ] {
        assert_eq!(
            instance
                .invoke_export(name, &[Value::I32(condition)])
                .expect("select call/return value context must execute"),
            Some(Value::I32(expected)),
            "unexpected result for {name} with condition {condition}"
        );
    }
}
