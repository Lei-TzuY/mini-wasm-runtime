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

fn memory_grow_zero_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(
        &mut module,
        1,
        &[
            0x03, 0x60, 0x02, I32, I32, 0x00, 0x60, 0x01, I32, 0x01, I32, 0x60, 0x00, 0x01, I32,
        ],
    );
    push_section(&mut module, 3, &[0x05, 0x00, 0x01, 0x01, 0x01, 0x02]);
    push_section(&mut module, 5, &[0x01, 0x01, 0x01, 0x03]);
    push_section(
        &mut module,
        7,
        &[
            0x05, 0x06, b's', b't', b'o', b'r', b'e', b'8', 0x00, 0x00, 0x05, b'l', b'o', b'a',
            b'd', b'8', 0x00, 0x01, 0x06, b'l', b'o', b'a', b'd', b'3', b'2', 0x00, 0x02, 0x04,
            b'g', b'r', b'o', b'w', 0x00, 0x03, 0x04, b's', b'i', b'z', b'e', 0x00, 0x04,
        ],
    );

    let mut code = vec![0x05];
    push_body(&mut code, &[0x20, 0x00, 0x20, 0x01, 0x3a, 0x00, 0x00]);
    push_body(&mut code, &[0x20, 0x00, 0x2d, 0x00, 0x00]);
    push_body(&mut code, &[0x20, 0x00, 0x28, 0x02, 0x00]);
    push_body(&mut code, &[0x20, 0x00, 0x40, 0x00]);
    push_body(&mut code, &[0x3f, 0x00]);
    push_section(&mut module, 10, &code);
    module
}

fn instance() -> Instance {
    Instance::new(
        parse_module(&memory_grow_zero_module()).expect("memory.grow zero-fill vector must parse"),
    )
    .expect("memory.grow zero-fill vector must validate and instantiate")
}

fn invoke_i32(vm: &mut Instance, name: &str, args: &[Value]) -> i32 {
    match vm
        .invoke_export(name, args)
        .expect("memory.grow zero-fill vector must execute")
    {
        Some(Value::I32(value)) => value,
        other => panic!("memory.grow zero-fill vector returned wrong value: {other:?}"),
    }
}

fn store8(vm: &mut Instance, address: usize, value: i32) {
    assert_eq!(
        vm.invoke_export("store8", &[Value::I32(address as i32), Value::I32(value)],)
            .expect("store8 must execute"),
        None
    );
}

fn load8(vm: &mut Instance, address: usize) -> i32 {
    invoke_i32(vm, "load8", &[Value::I32(address as i32)])
}

fn load32(vm: &mut Instance, address: usize) -> i32 {
    invoke_i32(vm, "load32", &[Value::I32(address as i32)])
}

#[test]
fn upstream_grow_zero_initializes_new_page_and_preserves_old_page() {
    // WebAssembly/spec test/core/memory_grow.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let mut vm = instance();
    store8(&mut vm, 0, 0x5a);
    store8(&mut vm, WASM_PAGE_SIZE - 1, 0xa5);

    assert_eq!(invoke_i32(&mut vm, "grow", &[Value::I32(1)]), 1);
    assert_eq!(invoke_i32(&mut vm, "size", &[]), 2);

    assert_eq!(load8(&mut vm, 0), 0x5a);
    assert_eq!(load8(&mut vm, WASM_PAGE_SIZE - 1), 0xa5);
    for address in [
        WASM_PAGE_SIZE,
        WASM_PAGE_SIZE + 12_345,
        2 * WASM_PAGE_SIZE - 1,
    ] {
        assert_eq!(load8(&mut vm, address), 0);
    }
}

#[test]
fn successive_grows_zero_each_new_page_without_clobbering_prior_growth() {
    let mut vm = instance();

    assert_eq!(invoke_i32(&mut vm, "grow", &[Value::I32(1)]), 1);
    store8(&mut vm, WASM_PAGE_SIZE + 17, 0x7b);

    assert_eq!(invoke_i32(&mut vm, "grow", &[Value::I32(1)]), 2);
    assert_eq!(invoke_i32(&mut vm, "size", &[]), 3);

    assert_eq!(load8(&mut vm, WASM_PAGE_SIZE + 17), 0x7b);
    assert_eq!(load8(&mut vm, 2 * WASM_PAGE_SIZE), 0);
    assert_eq!(load8(&mut vm, 3 * WASM_PAGE_SIZE - 1), 0);
}

#[test]
fn load_crossing_growth_boundary_observes_zero_filled_new_bytes() {
    let mut vm = instance();
    store8(&mut vm, WASM_PAGE_SIZE - 2, 0x12);
    store8(&mut vm, WASM_PAGE_SIZE - 1, 0x34);

    assert_eq!(invoke_i32(&mut vm, "grow", &[Value::I32(1)]), 1);
    assert_eq!(load32(&mut vm, WASM_PAGE_SIZE - 2), 0x0000_3412);
}
