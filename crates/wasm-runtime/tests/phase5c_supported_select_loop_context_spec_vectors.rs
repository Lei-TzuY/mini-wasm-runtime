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

fn select_loop_context_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(
        &mut module,
        1,
        &[
            0x02, // two types
            0x60, 0x00, 0x00, // type 0: () -> ()
            0x60, 0x01, I32, 0x01, I32, // type 1: (i32) -> i32
        ],
    );
    push_section(&mut module, 3, &[0x04, 0x00, 0x01, 0x01, 0x01]);

    let exports = [
        ("as-loop-first", 1),
        ("as-loop-mid", 2),
        ("as-loop-last", 3),
    ];
    let mut export_section = Vec::new();
    push_u32(&mut export_section, exports.len() as u32);
    for (name, function_index) in exports {
        push_export(&mut export_section, name, function_index);
    }
    push_section(&mut module, 7, &export_section);

    let mut code = vec![0x04];
    push_body(&mut code, &[]); // dummy
    push_body(
        &mut code,
        &[
            0x03, I32, // loop (result i32)
            0x41, 0x02, // first candidate: 2
            0x41, 0x03, // second candidate: 3
            0x20, 0x00, // condition
            0x1b, // select before both calls
            0x10, 0x00, // call dummy
            0x10, 0x00, // call dummy
            0x0b, // end loop
        ],
    );
    push_body(
        &mut code,
        &[
            0x03, I32, // loop (result i32)
            0x10, 0x00, // call dummy
            0x41, 0x02, // first candidate: 2
            0x41, 0x03, // second candidate: 3
            0x20, 0x00, // condition
            0x1b, // select between calls
            0x10, 0x00, // call dummy
            0x0b, // end loop
        ],
    );
    push_body(
        &mut code,
        &[
            0x03, I32, // loop (result i32)
            0x10, 0x00, // call dummy
            0x10, 0x00, // call dummy
            0x41, 0x02, // first candidate: 2
            0x41, 0x03, // second candidate: 3
            0x20, 0x00, // condition
            0x1b, // select after both calls
            0x0b, // end loop
        ],
    );
    push_section(&mut module, 10, &code);

    module
}

#[test]
fn pinned_upstream_select_loop_positions_execute() {
    // WebAssembly/spec test/core/select.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let module =
        parse_module(&select_loop_context_module()).expect("select loop context vector must parse");
    validate(&module).expect("select loop context vector must validate");
    let mut instance = Instance::new(module).expect("select loop context vector must instantiate");

    for (name, condition, expected) in [
        ("as-loop-first", 0, 3),
        ("as-loop-first", 1, 2),
        ("as-loop-mid", 0, 3),
        ("as-loop-mid", 1, 2),
        ("as-loop-last", 0, 3),
        ("as-loop-last", 1, 2),
    ] {
        assert_eq!(
            instance
                .invoke_export(name, &[Value::I32(condition)])
                .expect("select loop context must execute"),
            Some(Value::I32(expected)),
            "unexpected result for {name} with condition {condition}"
        );
    }
}
