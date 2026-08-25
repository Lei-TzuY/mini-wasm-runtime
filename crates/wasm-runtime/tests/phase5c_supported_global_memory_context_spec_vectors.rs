use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};

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

fn global_memory_context_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(
        &mut module,
        1,
        &[
            0x02, // two types
            0x60, 0x00, 0x00, // [] -> []
            0x60, 0x00, 0x01, I32, // [] -> i32
        ],
    );
    push_section(&mut module, 3, &[0x04, 0x00, 0x00, 0x01, 0x01]);
    push_section(&mut module, 5, &[0x01, 0x00, 0x01]); // (memory 1)
    push_section(
        &mut module,
        6,
        &[
            0x01, // one global
            I32, 0x01, // mutable i32
            0x41, 0x06, 0x0b, // i32.const 6; end
        ],
    );

    let exports = [
        ("as-store-first", 0),
        ("as-store-last", 1),
        ("as-load-operand", 2),
        ("as-memory.grow-value", 3),
    ];
    let mut export_section = Vec::new();
    push_u32(&mut export_section, exports.len() as u32);
    for (name, index) in exports {
        push_export(&mut export_section, name, index);
    }
    push_section(&mut module, 7, &export_section);

    let mut code = vec![0x04];
    push_body(
        &mut code,
        &[
            0x23, 0x00, // global.get $x (address 6)
            0x41, 0x01, // i32.const 1
            0x36, 0x02, 0x00, // i32.store align=2 offset=0
        ],
    );
    push_body(
        &mut code,
        &[
            0x41, 0x00, // i32.const 0
            0x23, 0x00, // global.get $x (value 6)
            0x36, 0x02, 0x00, // i32.store align=2 offset=0
        ],
    );
    push_body(
        &mut code,
        &[
            0x23, 0x00, // global.get $x (address 6)
            0x28, 0x02, 0x00, // i32.load align=2 offset=0
        ],
    );
    push_body(
        &mut code,
        &[
            0x23, 0x00, // global.get $x (delta 6)
            0x40, 0x00, // memory.grow 0
        ],
    );
    push_section(&mut module, 10, &code);

    module
}

#[test]
fn upstream_global_get_memory_context_vectors_execute() {
    // WebAssembly/spec test/core/global.wast @ the pinned revision. The
    // upstream sequence has already set $x to 6 before these assertions.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let module = parse_module(&global_memory_context_module())
        .expect("supported global memory context vector must parse");
    let mut vm = Instance::new(module).expect("supported global memory context must instantiate");

    assert_eq!(
        vm.invoke_export("as-store-first", &[])
            .expect("global address store must execute"),
        None
    );
    assert_eq!(
        vm.invoke_export("as-store-last", &[])
            .expect("global value store must execute"),
        None
    );
    assert_eq!(
        vm.invoke_export("as-load-operand", &[])
            .expect("global address load must execute"),
        Some(Value::I32(1))
    );
    assert_eq!(
        vm.invoke_export("as-memory.grow-value", &[])
            .expect("global memory.grow delta must execute"),
        Some(Value::I32(1))
    );
}
