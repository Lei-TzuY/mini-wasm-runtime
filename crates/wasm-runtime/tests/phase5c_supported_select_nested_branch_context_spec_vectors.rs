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

fn select_nested_branch_context_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(&mut module, 1, &[0x01, 0x60, 0x01, I32, 0x01, I32]);
    push_section(&mut module, 3, &[0x05, 0x00, 0x00, 0x00, 0x00, 0x00]);

    let exports = [
        ("as-select-first", 0),
        ("as-select-mid", 1),
        ("as-select-last", 2),
        ("as-br_if-first", 3),
        ("as-br_if-last", 4),
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
            0x41, 0x00, // inner first: 0
            0x41, 0x01, // inner second: 1
            0x20, 0x00, // inner condition
            0x1b, // inner select
            0x41, 0x02, // outer second: 2
            0x41, 0x03, // outer condition: nonzero
            0x1b, // outer select
        ],
    );
    push_body(
        &mut code,
        &[
            0x41, 0x02, // outer first: 2
            0x41, 0x00, // inner first: 0
            0x41, 0x01, // inner second: 1
            0x20, 0x00, // inner condition
            0x1b, // inner select
            0x41, 0x03, // outer condition: nonzero
            0x1b, // outer select
        ],
    );
    push_body(
        &mut code,
        &[
            0x41, 0x02, // outer first: 2
            0x41, 0x03, // outer second: 3
            0x41, 0x00, // inner first: 0
            0x41, 0x01, // inner second: 1
            0x20, 0x00, // inner condition
            0x1b, // inner select becomes outer condition
            0x1b, // outer select
        ],
    );
    push_body(
        &mut code,
        &[
            0x02, I32, // block (result i32)
            0x41, 0x02, // branch value first candidate: 2
            0x41, 0x03, // branch value second candidate: 3
            0x20, 0x00, // select condition
            0x1b, // selected branch value
            0x41, 0x04, // br_if condition: nonzero
            0x0d, 0x00, // br_if 0
            0x0b, // end block
        ],
    );
    push_body(
        &mut code,
        &[
            0x02, I32, // block (result i32)
            0x41, 0x02, // branch result value
            0x41, 0x02, // condition first candidate: 2
            0x41, 0x03, // condition second candidate: 3
            0x20, 0x00, // select condition
            0x1b, // selected br_if condition, always nonzero
            0x0d, 0x00, // br_if 0
            0x0b, // end block
        ],
    );
    push_section(&mut module, 10, &code);

    module
}

#[test]
fn pinned_upstream_nested_select_and_br_if_contexts_execute() {
    // WebAssembly/spec test/core/select.wast @ the pinned revision exercises
    // nested select positions and select as both the br_if label value and condition.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let module = parse_module(&select_nested_branch_context_module())
        .expect("select nested/branch context vector must parse");
    validate(&module).expect("select nested/branch context vector must validate");
    let mut instance =
        Instance::new(module).expect("select nested/branch context vector must instantiate");

    for (name, condition, expected) in [
        ("as-select-first", 0, 1),
        ("as-select-first", 1, 0),
        ("as-select-mid", 0, 2),
        ("as-select-mid", 1, 2),
        ("as-select-last", 0, 2),
        ("as-select-last", 1, 3),
        ("as-br_if-first", 0, 3),
        ("as-br_if-first", 1, 2),
        ("as-br_if-last", 0, 2),
        ("as-br_if-last", 1, 2),
    ] {
        assert_eq!(
            instance
                .invoke_export(name, &[Value::I32(condition)])
                .expect("select nested/branch context must execute"),
            Some(Value::I32(expected)),
            "unexpected result for {name} with condition {condition}"
        );
    }
}
