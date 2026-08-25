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

fn push_body(payload: &mut Vec<u8>, i32_locals: u32, instructions: &[u8]) {
    let mut body = Vec::new();
    if i32_locals == 0 {
        body.push(0x00);
    } else {
        body.push(0x01);
        push_u32(&mut body, i32_locals);
        body.push(I32);
    }
    body.extend_from_slice(instructions);
    body.push(0x0b);

    push_u32(payload, body.len() as u32);
    payload.extend_from_slice(&body);
}

fn select_state_context_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(
        &mut module,
        1,
        &[
            0x01, // one type
            0x60, 0x01, I32, 0x01, I32, // (i32) -> i32
        ],
    );
    push_section(&mut module, 3, &[0x03, 0x00, 0x00, 0x00]);
    push_section(
        &mut module,
        6,
        &[
            0x01, // one global
            I32, 0x01, // mutable i32
            0x41, 0x0a, 0x0b, // i32.const 10; end
        ],
    );

    let exports = [
        ("as-local.set-value", 0),
        ("as-local.tee-value", 1),
        ("as-global.set-value", 2),
    ];
    let mut export_section = Vec::new();
    push_u32(&mut export_section, exports.len() as u32);
    for (name, index) in exports {
        push_export(&mut export_section, name, index);
    }
    push_section(&mut module, 7, &export_section);

    let mut code = vec![0x03];
    push_body(
        &mut code,
        1,
        &[
            0x41, 0x01, // i32.const 1
            0x41, 0x02, // i32.const 2
            0x20, 0x00, // local.get 0 (condition)
            0x1b, // select
            0x21, 0x00, // local.set 0
            0x20, 0x00, // local.get 0
        ],
    );
    push_body(
        &mut code,
        1,
        &[
            0x41, 0x01, // i32.const 1
            0x41, 0x02, // i32.const 2
            0x20, 0x00, // local.get 0 (condition)
            0x1b, // select
            0x22, 0x00, // local.tee 0
        ],
    );
    push_body(
        &mut code,
        0,
        &[
            0x41, 0x01, // i32.const 1
            0x41, 0x02, // i32.const 2
            0x20, 0x00, // local.get 0 (condition)
            0x1b, // select
            0x24, 0x00, // global.set 0
            0x23, 0x00, // global.get 0
        ],
    );
    push_section(&mut module, 10, &code);

    module
}

#[test]
fn upstream_select_state_assignment_contexts_execute() {
    // WebAssembly/spec test/core/select.wast @ the pinned revision exercises
    // select as the value of local.set, local.tee, and global.set.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let module = parse_module(&select_state_context_module())
        .expect("select state-context vector must parse");
    validate(&module).expect("select state-context vector must validate");
    let mut instance = Instance::new(module).expect("select state-context vector must instantiate");

    for (condition, expected) in [(0, 2), (1, 1)] {
        for name in [
            "as-local.set-value",
            "as-local.tee-value",
            "as-global.set-value",
        ] {
            assert_eq!(
                instance
                    .invoke_export(name, &[Value::I32(condition)])
                    .expect("select state-context vector must execute"),
                Some(Value::I32(expected)),
                "unexpected result for {name} with condition {condition}"
            );
        }
    }
}
