use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};

const I32: u8 = 0x7f;
const I64: u8 = 0x7e;
const F32: u8 = 0x7d;
const F64: u8 = 0x7c;
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

fn push_export(payload: &mut Vec<u8>, name: &str, function_index: u8) {
    push_name(payload, name);
    payload.extend([0x00, function_index]);
}

fn call_indirect_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    let types = [
        0x06, 0x60, 0x01, I32, 0x01, I32, 0x60, 0x01, I32, 0x01, I32, 0x60, 0x02, I32, I32, 0x01,
        I32, 0x60, 0x01, I64, 0x01, I64, 0x60, 0x01, F32, 0x01, F32, 0x60, 0x01, F64, 0x01, F64,
    ];
    push_section(&mut module, 1, &types);

    push_section(
        &mut module,
        3,
        &[
            0x0a, 0x00, 0x00, 0x02, 0x00, 0x03, 0x03, 0x04, 0x04, 0x05, 0x05,
        ],
    );
    push_section(&mut module, 4, &[0x01, 0x70, 0x00, 0x05]);

    let mut exports = vec![0x05];
    push_export(&mut exports, "dispatch", 2);
    push_export(&mut exports, "structural", 3);
    push_export(&mut exports, "id-i64", 5);
    push_export(&mut exports, "id-f32", 7);
    push_export(&mut exports, "id-f64", 9);
    push_section(&mut module, 7, &exports);

    push_section(
        &mut module,
        9,
        &[
            0x01, 0x00, 0x41, 0x00, 0x0b, 0x05, 0x00, 0x01, 0x04, 0x06, 0x08,
        ],
    );

    let mut code = vec![0x0a];
    push_body(&mut code, &[0x20, 0x00, 0x41, 0x01, 0x6a]);
    push_body(&mut code, &[0x20, 0x00, 0x41, 0x02, 0x6a]);
    push_body(&mut code, &[0x20, 0x00, 0x20, 0x01, 0x11, 0x00, 0x00]);
    push_body(&mut code, &[0x20, 0x00, 0x41, 0x00, 0x11, 0x01, 0x00]);
    push_body(&mut code, &[0x20, 0x00]);
    push_body(&mut code, &[0x20, 0x00, 0x41, 0x02, 0x11, 0x03, 0x00]);
    push_body(&mut code, &[0x20, 0x00]);
    push_body(&mut code, &[0x20, 0x00, 0x41, 0x03, 0x11, 0x04, 0x00]);
    push_body(&mut code, &[0x20, 0x00]);
    push_body(&mut code, &[0x20, 0x00, 0x41, 0x04, 0x11, 0x05, 0x00]);
    push_section(&mut module, 10, &code);

    module
}

fn instance() -> Instance {
    Instance::new(
        parse_module(&call_indirect_module()).expect("call_indirect spec vector must parse"),
    )
    .expect("call_indirect spec vector must validate and instantiate")
}

#[test]
fn upstream_dynamic_dispatch_selects_initialized_table_slot() {
    // WebAssembly/spec test/core/call_indirect.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let mut vm = instance();
    assert_eq!(
        vm.invoke_export("dispatch", &[Value::I32(40), Value::I32(0)])
            .expect("slot 0 dispatch must execute"),
        Some(Value::I32(41))
    );
    assert_eq!(
        vm.invoke_export("dispatch", &[Value::I32(40), Value::I32(1)])
            .expect("slot 1 dispatch must execute"),
        Some(Value::I32(42))
    );
}

#[test]
fn upstream_structurally_equal_distinct_type_indices_match() {
    let mut vm = instance();
    assert_eq!(
        vm.invoke_export("structural", &[Value::I32(76)])
            .expect("structurally equal indirect signature must match"),
        Some(Value::I32(77))
    );
}

#[test]
fn upstream_non_i32_indirect_payloads_round_trip() {
    let mut vm = instance();

    assert_eq!(
        vm.invoke_export("id-i64", &[Value::I64(0x1234_5678_9abc_def0)])
            .expect("i64 indirect call must execute"),
        Some(Value::I64(0x1234_5678_9abc_def0))
    );
    assert_eq!(
        vm.invoke_export("id-f32", &[Value::F32(1.25)])
            .expect("f32 indirect call must execute"),
        Some(Value::F32(1.25))
    );
    assert_eq!(
        vm.invoke_export("id-f64", &[Value::F64(-3.5)])
            .expect("f64 indirect call must execute"),
        Some(Value::F64(-3.5))
    );
}
