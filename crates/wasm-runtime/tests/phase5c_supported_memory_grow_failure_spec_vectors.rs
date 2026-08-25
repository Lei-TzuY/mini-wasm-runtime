use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value, WASM_PAGE_SIZE};

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

fn push_body(payload: &mut Vec<u8>, instructions: &[u8]) {
    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    push_u32(payload, body.len() as u32);
    payload.extend_from_slice(&body);
}

fn grow_failure_module(max_pages: u8) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(
        &mut module,
        1,
        &[
            0x03, 0x60, 0x00, 0x01, I32, 0x60, 0x01, I32, 0x01, I32, 0x60, 0x02, I32, I32,
            0x00,
        ],
    );
    push_section(&mut module, 3, &[0x04, 0x00, 0x01, 0x01, 0x02]);
    push_section(&mut module, 5, &[0x01, 0x01, 0x01, max_pages]);
    push_section(
        &mut module,
        7,
        &[
            0x04, 0x04, b's', b'i', b'z', b'e', 0x00, 0x00, 0x04, b'g', b'r', b'o', b'w', 0x00,
            0x01, 0x04, b'l', b'o', b'a', b'd', 0x00, 0x02, 0x05, b's', b't', b'o', b'r', b'e',
            0x00, 0x03,
        ],
    );

    let mut code = vec![0x04];
    push_body(&mut code, &[0x3f, 0x00]);
    push_body(&mut code, &[0x20, 0x00, 0x40, 0x00]);
    push_body(&mut code, &[0x20, 0x00, 0x28, 0x02, 0x00]);
    push_body(&mut code, &[0x20, 0x00, 0x20, 0x01, 0x36, 0x02, 0x00]);
    push_section(&mut module, 10, &code);
    module
}

fn instance(max_pages: u8) -> Instance {
    Instance::new(
        parse_module(&grow_failure_module(max_pages))
            .expect("memory.grow failure vector must parse"),
    )
    .expect("memory.grow failure vector must validate and instantiate")
}

fn invoke_i32(vm: &mut Instance, name: &str, args: &[Value]) -> i32 {
    match vm
        .invoke_export(name, args)
        .expect("memory.grow failure vector must execute")
    {
        Some(Value::I32(value)) => value,
        other => panic!("memory.grow failure vector returned wrong value: {other:?}"),
    }
}

fn store(vm: &mut Instance, address: i32, value: i32) {
    assert_eq!(
        vm.invoke_export("store", &[Value::I32(address), Value::I32(value)])
            .expect("sentinel store must execute"),
        None
    );
}

fn load(vm: &mut Instance, address: i32) -> i32 {
    invoke_i32(vm, "load", &[Value::I32(address)])
}

#[test]
fn failed_growth_at_declared_max_preserves_size_and_existing_bytes() {
    // WebAssembly/spec test/core/memory_grow.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let mut vm = instance(1);
    let page_end = WASM_PAGE_SIZE as i32 - 4;
    store(&mut vm, 0, 0x1234_5678);
    store(&mut vm, page_end, 0x7654_3210);

    assert_eq!(invoke_i32(&mut vm, "size", &[]), 1);
    assert_eq!(invoke_i32(&mut vm, "grow", &[Value::I32(1)]), -1);
    assert_eq!(invoke_i32(&mut vm, "size", &[]), 1);
    assert_eq!(load(&mut vm, 0), 0x1234_5678);
    assert_eq!(load(&mut vm, page_end), 0x7654_3210);
}

#[test]
fn failed_oversized_growth_does_not_poison_later_legal_growth() {
    let mut vm = instance(2);
    let page_end = WASM_PAGE_SIZE as i32 - 4;
    store(&mut vm, 0, 0x1020_3040);
    store(&mut vm, page_end, 0x5060_7080);

    assert_eq!(invoke_i32(&mut vm, "grow", &[Value::I32(2)]), -1);
    assert_eq!(invoke_i32(&mut vm, "size", &[]), 1);
    assert_eq!(load(&mut vm, 0), 0x1020_3040);
    assert_eq!(load(&mut vm, page_end), 0x5060_7080);

    assert_eq!(invoke_i32(&mut vm, "grow", &[Value::I32(1)]), 1);
    assert_eq!(invoke_i32(&mut vm, "size", &[]), 2);
    assert_eq!(load(&mut vm, 0), 0x1020_3040);
    assert_eq!(load(&mut vm, page_end), 0x5060_7080);
    assert_eq!(load(&mut vm, WASM_PAGE_SIZE as i32), 0);
}
