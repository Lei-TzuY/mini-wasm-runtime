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

fn select_memory_context_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(
        &mut module,
        1,
        &[
            0x03, // three types
            0x60, 0x01, I32, 0x00, // type 0: (i32) -> ()
            0x60, 0x01, I32, 0x01, I32, // type 1: (i32) -> i32
            0x60, 0x00, 0x01, I32, // type 2: () -> i32
        ],
    );
    push_section(&mut module, 3, &[0x06, 0x00, 0x00, 0x01, 0x01, 0x02, 0x02]);
    push_section(&mut module, 5, &[0x01, 0x01, 0x01, 0x04]); // memory 1 4

    let exports = [
        ("as-store-first", 0),
        ("as-store-last", 1),
        ("as-load-operand", 2),
        ("as-memory.grow-value", 3),
        ("read-8", 4),
        ("memory-size", 5),
    ];
    let mut export_section = Vec::new();
    push_u32(&mut export_section, exports.len() as u32);
    for (name, function_index) in exports {
        push_export(&mut export_section, name, function_index);
    }
    push_section(&mut module, 7, &export_section);

    let mut code = vec![0x06];
    push_body(
        &mut code,
        &[
            0x41, 0x00, // first address candidate: 0
            0x41, 0x04, // second address candidate: 4
            0x20, 0x00, // local.get condition
            0x1b, // select address
            0x41, 0x01, // value 1
            0x36, 0x02, 0x00, // i32.store align=2 offset=0
        ],
    );
    push_body(
        &mut code,
        &[
            0x41, 0x08, // address 8
            0x41, 0x01, // first value candidate: 1
            0x41, 0x02, // second value candidate: 2
            0x20, 0x00, // local.get condition
            0x1b, // select value
            0x36, 0x02, 0x00, // i32.store align=2 offset=0
        ],
    );
    push_body(
        &mut code,
        &[
            0x41, 0x00, // first address candidate: 0
            0x41, 0x04, // second address candidate: 4
            0x20, 0x00, // local.get condition
            0x1b, // select address
            0x28, 0x02, 0x00, // i32.load align=2 offset=0
        ],
    );
    push_body(
        &mut code,
        &[
            0x41, 0x01, // first delta candidate: 1
            0x41, 0x02, // second delta candidate: 2
            0x20, 0x00, // local.get condition
            0x1b, // select delta
            0x40, 0x00, // memory.grow 0
        ],
    );
    push_body(
        &mut code,
        &[
            0x41, 0x08, // address 8
            0x28, 0x02, 0x00, // i32.load align=2 offset=0
        ],
    );
    push_body(
        &mut code,
        &[
            0x3f, 0x00, // memory.size 0
        ],
    );
    push_section(&mut module, 10, &code);

    module
}

fn instance() -> Instance {
    let module = parse_module(&select_memory_context_module())
        .expect("select memory context vector must parse");
    validate(&module).expect("select memory context vector must validate");
    Instance::new(module).expect("select memory context vector must instantiate")
}

#[test]
fn pinned_upstream_select_load_store_contexts_execute() {
    // WebAssembly/spec test/core/select.wast @ the pinned revision exercises
    // select as both an i32.store address/value operand and an i32.load address.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);
    let mut vm = instance();

    assert_eq!(
        vm.invoke_export("as-store-first", &[Value::I32(1)])
            .expect("selected address 0 store must execute"),
        None
    );
    assert_eq!(
        vm.invoke_export("as-load-operand", &[Value::I32(1)])
            .expect("selected address 0 load must execute"),
        Some(Value::I32(1))
    );

    assert_eq!(
        vm.invoke_export("as-store-first", &[Value::I32(0)])
            .expect("selected address 4 store must execute"),
        None
    );
    assert_eq!(
        vm.invoke_export("as-load-operand", &[Value::I32(0)])
            .expect("selected address 4 load must execute"),
        Some(Value::I32(1))
    );

    assert_eq!(
        vm.invoke_export("as-store-last", &[Value::I32(1)])
            .expect("selected value 1 store must execute"),
        None
    );
    assert_eq!(
        vm.invoke_export("read-8", &[])
            .expect("stored selected value must be observable"),
        Some(Value::I32(1))
    );

    assert_eq!(
        vm.invoke_export("as-store-last", &[Value::I32(0)])
            .expect("selected value 2 store must execute"),
        None
    );
    assert_eq!(
        vm.invoke_export("read-8", &[])
            .expect("overwritten selected value must be observable"),
        Some(Value::I32(2))
    );
}

#[test]
fn pinned_upstream_select_memory_grow_context_executes_both_directions() {
    for (condition, expected_pages) in [(1, 2), (0, 3)] {
        let mut vm = instance();
        assert_eq!(
            vm.invoke_export("as-memory.grow-value", &[Value::I32(condition)])
                .expect("selected memory.grow delta must execute"),
            Some(Value::I32(1))
        );
        assert_eq!(
            vm.invoke_export("memory-size", &[])
                .expect("grown memory size must be observable"),
            Some(Value::I32(expected_pages)),
            "unexpected memory size for select condition {condition}"
        );
    }
}
