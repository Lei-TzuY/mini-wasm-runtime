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

fn push_body(payload: &mut Vec<u8>, instructions: &[u8]) {
    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    push_u32(payload, body.len() as u32);
    payload.extend_from_slice(&body);
}

fn select_if_condition_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(
        &mut module,
        1,
        &[
            0x02, // two types
            0x60, 0x00, 0x00, // type 0: () -> ()
            0x60, 0x01, I32, 0x00, // type 1: (i32) -> ()
        ],
    );
    push_section(&mut module, 3, &[0x02, 0x00, 0x01]);

    let mut exports = vec![0x01];
    push_name(&mut exports, "as-if-condition");
    exports.push(0x00);
    push_u32(&mut exports, 1);
    push_section(&mut module, 7, &exports);

    let mut code = vec![0x02];
    push_body(&mut code, &[]);
    push_body(
        &mut code,
        &[
            0x41, 0x02, // first condition candidate: 2
            0x41, 0x03, // second condition candidate: 3
            0x20, 0x00, // local.get selector
            0x1b, // select: both upstream candidates are nonzero
            0x04, 0x40, // if (no result)
            0x10, 0x00, // then: call dummy
            0x0b, // end if
        ],
    );
    push_section(&mut module, 10, &code);

    module
}

#[test]
fn pinned_upstream_select_if_condition_executes_both_nonzero_directions() {
    // WebAssembly/spec test/core/select.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let module = parse_module(&select_if_condition_module())
        .expect("select if-condition vector must parse");
    validate(&module).expect("select if-condition vector must validate");
    let mut instance =
        Instance::new(module).expect("select if-condition vector must instantiate");

    for condition in [0, 1] {
        assert_eq!(
            instance
                .invoke_export("as-if-condition", &[Value::I32(condition)])
                .expect("select if-condition context must execute"),
            None,
            "unexpected result for upstream selector {condition}"
        );
    }
}
