use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};
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

fn select_call_indirect_context_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(
        &mut module,
        1,
        &[
            0x02, // two types
            0x60, 0x02, I32, I32, 0x01, I32, // type 0: (i32, i32) -> i32
            0x60, 0x01, I32, 0x01, I32, // type 1: (i32) -> i32
        ],
    );
    push_section(&mut module, 3, &[0x04, 0x00, 0x01, 0x01, 0x01]);
    push_section(&mut module, 4, &[0x01, 0x70, 0x00, 0x01]); // table 1 funcref

    let exports = [
        ("as-call_indirect-first", 1),
        ("as-call_indirect-mid", 2),
        ("as-call_indirect-last", 3),
    ];
    let mut export_section = Vec::new();
    push_u32(&mut export_section, exports.len() as u32);
    for (name, function_index) in exports {
        push_export(&mut export_section, name, function_index);
    }
    push_section(&mut module, 7, &export_section);

    push_section(
        &mut module,
        9,
        &[
            0x01, // one active element segment
            0x00, // legacy active mode 0
            0x41, 0x00, 0x0b, // i32.const 0; end
            0x01, 0x00, // one function: function 0
        ],
    );

    let mut code = vec![0x04];
    push_body(&mut code, &[0x20, 0x00]); // target returns its first argument
    push_body(
        &mut code,
        &[
            0x41, 0x02, // first arg candidate: 2
            0x41, 0x03, // first arg candidate: 3
            0x20, 0x00, // condition
            0x1b, // select first argument
            0x41, 0x01, // second argument: 1
            0x41, 0x00, // table element index: 0
            0x11, 0x00, 0x00, // call_indirect type 0, table 0
        ],
    );
    push_body(
        &mut code,
        &[
            0x41, 0x01, // first argument: 1
            0x41, 0x02, // second arg candidate: 2
            0x41, 0x03, // second arg candidate: 3
            0x20, 0x00, // condition
            0x1b, // select second argument
            0x41, 0x00, // table element index: 0
            0x11, 0x00, 0x00, // call_indirect type 0, table 0
        ],
    );
    push_body(
        &mut code,
        &[
            0x41, 0x01, // first argument: 1
            0x41, 0x04, // second argument: 4
            0x41, 0x02, // table-index candidate: 2
            0x41, 0x03, // table-index candidate: 3
            0x20, 0x00, // condition
            0x1b, // select table element index
            0x11, 0x00, 0x00, // call_indirect type 0, table 0
        ],
    );
    push_section(&mut module, 10, &code);

    module
}

fn instance() -> Instance {
    let module = parse_module(&select_call_indirect_context_module())
        .expect("select call_indirect context vector must parse");
    validate(&module).expect("select call_indirect context vector must validate");
    Instance::new(module).expect("select call_indirect context vector must instantiate")
}

#[test]
fn pinned_upstream_select_call_indirect_argument_contexts_execute() {
    // WebAssembly/spec test/core/select.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);
    let mut vm = instance();

    for (name, condition, expected) in [
        ("as-call_indirect-first", 0, 3),
        ("as-call_indirect-first", 1, 2),
        ("as-call_indirect-mid", 0, 1),
        ("as-call_indirect-mid", 1, 1),
    ] {
        assert_eq!(
            vm.invoke_export(name, &[Value::I32(condition)])
                .expect("selected call_indirect argument must execute"),
            Some(Value::I32(expected)),
            "unexpected result for {name} with condition {condition}"
        );
    }
}

#[test]
fn pinned_upstream_select_call_indirect_table_index_context_traps() {
    let mut vm = instance();

    for (condition, expected_index) in [(0, 3), (1, 2)] {
        match vm.invoke_export("as-call_indirect-last", &[Value::I32(condition)]) {
            Err(RuntimeError::TableElementOutOfBounds(index)) => assert_eq!(index, expected_index),
            other => panic!(
                "unexpected trap for selected table index with condition {condition}: {other:?}"
            ),
        }
    }
}
