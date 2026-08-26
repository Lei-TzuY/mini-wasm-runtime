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

fn select_test_operand_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(&mut module, 1, &[0x01, 0x60, 0x01, I32, 0x01, I32]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(
        &mut module,
        7,
        &[
            0x01, 0x0f, b'a', b's', b'-', b't', b'e', b's', b't', b'-', b'o', b'p', b'e', b'r',
            b'a', b'n', b'd', 0x00, 0x00,
        ],
    );

    let instructions = [
        0x02, I32, // block (result i32)
        0x41, 0x00, // select first candidate: 0
        0x41, 0x01, // select second candidate: 1
        0x20, 0x00, // local.get 0: select condition
        0x1b, // select
        0x45, // i32.eqz
        0x0b, // end result block
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
fn pinned_upstream_select_test_operand_context_executes_both_directions() {
    // WebAssembly/spec test/core/select.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let module = parse_module(&select_test_operand_module())
        .expect("select test-operand context vector must parse");
    validate(&module).expect("select test-operand context vector must validate");
    let mut instance =
        Instance::new(module).expect("select test-operand context vector must instantiate");

    for (condition, expected) in [(0, 0), (1, 1)] {
        assert_eq!(
            instance
                .invoke_export("as-test-operand", &[Value::I32(condition)])
                .expect("select test-operand context must execute"),
            Some(Value::I32(expected)),
            "unexpected as-test-operand result for condition {condition}"
        );
    }
}
