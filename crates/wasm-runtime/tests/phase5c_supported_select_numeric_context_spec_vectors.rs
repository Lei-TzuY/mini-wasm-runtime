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

fn select_numeric_context_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(
        &mut module,
        1,
        &[
            0x01, // one type
            0x60, 0x01, I32, 0x01, I32, // (i32) -> i32
        ],
    );
    push_section(&mut module, 3, &[0x05, 0x00, 0x00, 0x00, 0x00, 0x00]);

    let exports = [
        ("as-unary-operand", 0),
        ("as-binary-operand", 1),
        ("as-compare-left", 2),
        ("as-compare-right", 3),
        ("as-convert-operand", 4),
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
            0x41, 0x00, // first candidate: 0
            0x41, 0x01, // second candidate: 1
            0x20, 0x00, // local.get condition
            0x1b, // select
            0x45, // i32.eqz
        ],
    );
    push_body(
        &mut code,
        &[
            0x41, 0x01, // first lhs candidate: 1
            0x41, 0x02, // second lhs candidate: 2
            0x20, 0x00, // local.get condition
            0x1b, // select lhs
            0x41, 0x01, // first rhs candidate: 1
            0x41, 0x02, // second rhs candidate: 2
            0x20, 0x00, // local.get condition
            0x1b, // select rhs
            0x6c, // i32.mul
        ],
    );
    push_body(
        &mut code,
        &[
            0x41, 0x01, // first candidate: 1
            0x41, 0x02, // second candidate: 2
            0x20, 0x00, // local.get condition
            0x1b, // select left operand
            0x41, 0x01, // compare against 1
            0x4c, // i32.le_s
        ],
    );
    push_body(
        &mut code,
        &[
            0x41, 0x01, // left operand: 1
            0x41, 0x00, // first right candidate: 0
            0x41, 0x01, // second right candidate: 1
            0x20, 0x00, // local.get condition
            0x1b, // select right operand
            0x47, // i32.ne
        ],
    );
    push_body(
        &mut code,
        &[
            0x42, 0x01, // i64.const 1
            0x42, 0x00, // i64.const 0
            0x20, 0x00, // local.get condition
            0x1b, // select i64 operand
            0xa7, // i32.wrap_i64
        ],
    );
    push_section(&mut module, 10, &code);

    module
}

#[test]
fn pinned_upstream_select_numeric_operand_contexts_execute() {
    // WebAssembly/spec test/core/select.wast @ the pinned revision exercises
    // select as unary, binary, comparison, and conversion operands.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let module = parse_module(&select_numeric_context_module())
        .expect("select numeric context vector must parse");
    validate(&module).expect("select numeric context vector must validate");
    let mut instance =
        Instance::new(module).expect("select numeric context vector must instantiate");

    for (name, condition, expected) in [
        ("as-unary-operand", 0, 0),
        ("as-unary-operand", 1, 1),
        ("as-binary-operand", 0, 4),
        ("as-binary-operand", 1, 1),
        ("as-compare-left", 0, 0),
        ("as-compare-left", 1, 1),
        ("as-compare-right", 0, 0),
        ("as-compare-right", 1, 1),
        ("as-convert-operand", 0, 0),
        ("as-convert-operand", 1, 1),
    ] {
        assert_eq!(
            instance
                .invoke_export(name, &[Value::I32(condition)])
                .expect("select numeric context must execute"),
            Some(Value::I32(expected)),
            "unexpected result for {name} with condition {condition}"
        );
    }
}
