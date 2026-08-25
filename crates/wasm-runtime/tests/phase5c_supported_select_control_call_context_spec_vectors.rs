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

fn select_control_call_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(
        &mut module,
        1,
        &[
            0x02, // two types
            0x60, 0x01, I32, 0x01, I32, // type 0: (i32) -> i32
            0x60, 0x00, 0x01, I32, // type 1: () -> i32
        ],
    );
    push_section(&mut module, 3, &[0x05, 0x00, 0x01, 0x01, 0x01, 0x01]);

    let exports = [
        ("as-return-value", 1),
        ("as-call-operand", 2),
        ("as-if-condition-false", 3),
        ("as-if-condition-true", 4),
    ];
    let mut export_section = Vec::new();
    push_u32(&mut export_section, exports.len() as u32);
    for (name, function_index) in exports {
        push_export(&mut export_section, name, function_index);
    }
    push_section(&mut module, 7, &export_section);

    let mut code = vec![0x05];
    push_body(
        &mut code,
        &[
            0x20, 0x00, // local.get 0
        ],
    );
    push_body(
        &mut code,
        &[
            0x41, 0x0b, // i32.const 11
            0x41, 0x16, // i32.const 22
            0x41, 0x01, // i32.const 1
            0x1b, // select => 11
            0x0f, // return
        ],
    );
    push_body(
        &mut code,
        &[
            0x41, 0x0b, // i32.const 11
            0x41, 0x16, // i32.const 22
            0x41, 0x00, // i32.const 0
            0x1b, // select => 22
            0x10, 0x00, // call identity
        ],
    );
    push_body(
        &mut code,
        &[
            0x41, 0x00, // first condition candidate: false
            0x41, 0x01, // second condition candidate: true
            0x41, 0x01, // choose first => false
            0x1b, // select
            0x04, I32, // if (result i32)
            0x41, 0x21, // then: 33
            0x05, // else
            0x41, 0x2c, // else: 44
            0x0b,
        ],
    );
    push_body(
        &mut code,
        &[
            0x41, 0x00, // first condition candidate: false
            0x41, 0x01, // second condition candidate: true
            0x41, 0x00, // choose second => true
            0x1b, // select
            0x04, I32, // if (result i32)
            0x41, 0x21, // then: 33
            0x05, // else
            0x41, 0x2c, // else: 44
            0x0b,
        ],
    );
    push_section(&mut module, 10, &code);

    module
}

#[test]
fn pinned_upstream_select_control_and_call_contexts_execute() {
    // WebAssembly/spec test/core/select.wast @ the pinned revision exercises
    // select values in return, call-operand, and conditional-control contexts.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let module = parse_module(&select_control_call_module())
        .expect("select control/call context vector must parse");
    validate(&module).expect("select control/call context vector must validate");
    let mut instance =
        Instance::new(module).expect("select control/call context vector must instantiate");

    for (name, expected) in [
        ("as-return-value", 11),
        ("as-call-operand", 22),
        ("as-if-condition-false", 44),
        ("as-if-condition-true", 33),
    ] {
        assert_eq!(
            instance
                .invoke_export(name, &[])
                .expect("select control/call context must execute"),
            Some(Value::I32(expected)),
            "unexpected result for {name}"
        );
    }
}
