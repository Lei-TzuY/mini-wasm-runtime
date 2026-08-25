use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, RuntimeError, RuntimeLimits};

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

fn runaway_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x00]);
    push_section(&mut module, 3, &[0x03, 0x00, 0x00, 0x00]);
    push_section(&mut module, 4, &[0x01, 0x70, 0x00, 0x03]);

    let mut exports = vec![0x02];
    push_export(&mut exports, "runaway", 0);
    push_export(&mut exports, "mutual-runaway", 1);
    push_section(&mut module, 7, &exports);

    push_section(
        &mut module,
        9,
        &[0x01, 0x00, 0x41, 0x00, 0x0b, 0x03, 0x00, 0x01, 0x02],
    );

    let mut code = vec![0x03];
    push_body(&mut code, &[0x41, 0x00, 0x11, 0x00, 0x00]);
    push_body(&mut code, &[0x41, 0x02, 0x11, 0x00, 0x00]);
    push_body(&mut code, &[0x41, 0x01, 0x11, 0x00, 0x00]);
    push_section(&mut module, 10, &code);

    module
}

fn instance(max_call_depth: usize) -> Instance {
    let module =
        parse_module(&runaway_module()).expect("call_indirect exhaustion vector must parse");
    let limits = RuntimeLimits {
        max_call_depth,
        ..RuntimeLimits::default()
    };
    Instance::with_config(module, HostRegistry::new(), limits)
        .expect("call_indirect exhaustion vector must validate and instantiate")
}

#[test]
fn upstream_self_recursive_call_indirect_exhausts_configured_call_depth() {
    // WebAssembly/spec test/core/call_indirect.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let mut vm = instance(4);
    assert!(matches!(
        vm.invoke_export("runaway", &[]),
        Err(RuntimeError::CallDepthExceeded { limit: 4 })
    ));
}

#[test]
fn upstream_mutual_recursive_call_indirect_exhausts_configured_call_depth() {
    let mut vm = instance(5);
    assert!(matches!(
        vm.invoke_export("mutual-runaway", &[]),
        Err(RuntimeError::CallDepthExceeded { limit: 5 })
    ));
}
