use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value, WASM_PAGE_SIZE};

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

fn zero_page_transition_module() -> Vec<u8> {
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
    push_section(&mut module, 5, &[0x01, 0x01, 0x00, 0x02]);
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

fn instance() -> Instance {
    Instance::new(
        parse_module(&zero_page_transition_module())
            .expect("zero-page memory transition vector must parse"),
    )
    .expect("zero-page memory transition vector must validate and instantiate")
}

fn invoke_i32(vm: &mut Instance, name: &str, args: &[Value]) -> i32 {
    match vm
        .invoke_export(name, args)
        .expect("zero-page memory transition vector must execute")
    {
        Some(Value::I32(value)) => value,
        other => panic!("zero-page memory transition vector returned wrong value: {other:?}"),
    }
}

fn assert_memory_oob(error: RuntimeError, expected_address: u64) {
    match error {
        RuntimeError::MemoryOutOfBounds { address, width } => {
            assert_eq!(address, expected_address);
            assert_eq!(width, 4);
        }
        other => panic!("expected precise memory out-of-bounds trap, got {other:?}"),
    }
}

#[test]
fn upstream_zero_page_memory_traps_until_first_growth() {
    // WebAssembly/spec test/core/memory_grow.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let mut vm = instance();
    assert_eq!(invoke_i32(&mut vm, "size", &[]), 0);

    let load_error = vm
        .invoke_export("load", &[Value::I32(0)])
        .expect_err("load from zero-page memory must trap");
    assert_memory_oob(load_error, 0);

    let store_error = vm
        .invoke_export("store", &[Value::I32(0), Value::I32(2)])
        .expect_err("store to zero-page memory must trap");
    assert_memory_oob(store_error, 0);

    assert_eq!(invoke_i32(&mut vm, "grow", &[Value::I32(1)]), 0);
    assert_eq!(invoke_i32(&mut vm, "size", &[]), 1);
    assert_eq!(invoke_i32(&mut vm, "load", &[Value::I32(0)]), 0);
    assert_eq!(
        vm.invoke_export("store", &[Value::I32(0), Value::I32(2)])
            .expect("store at zero must become legal after growth"),
        None
    );
    assert_eq!(invoke_i32(&mut vm, "load", &[Value::I32(0)]), 2);

    let next_page = WASM_PAGE_SIZE as i32;
    let load_error = vm
        .invoke_export("load", &[Value::I32(next_page)])
        .expect_err("the next page must remain out of bounds after a one-page growth");
    assert_memory_oob(load_error, WASM_PAGE_SIZE as u64);

    let store_error = vm
        .invoke_export("store", &[Value::I32(next_page), Value::I32(3)])
        .expect_err("the next page store must remain out of bounds after a one-page growth");
    assert_memory_oob(store_error, WASM_PAGE_SIZE as u64);
}

#[test]
fn second_growth_makes_previous_page_boundary_zero_filled_and_preserves_page_zero() {
    let mut vm = instance();
    assert_eq!(invoke_i32(&mut vm, "grow", &[Value::I32(1)]), 0);
    assert_eq!(
        vm.invoke_export("store", &[Value::I32(0), Value::I32(0x1234_5678)])
            .expect("page-zero store must execute after first growth"),
        None
    );

    let next_page = WASM_PAGE_SIZE as i32;
    let error = vm
        .invoke_export("load", &[Value::I32(next_page)])
        .expect_err("page-one start must trap before the second growth");
    assert_memory_oob(error, WASM_PAGE_SIZE as u64);

    assert_eq!(invoke_i32(&mut vm, "grow", &[Value::I32(1)]), 1);
    assert_eq!(invoke_i32(&mut vm, "size", &[]), 2);
    assert_eq!(invoke_i32(&mut vm, "load", &[Value::I32(next_page)]), 0);
    assert_eq!(
        invoke_i32(&mut vm, "load", &[Value::I32(0)]),
        0x1234_5678
    );

    assert_eq!(
        vm.invoke_export("store", &[Value::I32(next_page), Value::I32(3)])
            .expect("page-one store must become legal after the second growth"),
        None
    );
    assert_eq!(invoke_i32(&mut vm, "load", &[Value::I32(next_page)]), 3);
    assert_eq!(
        invoke_i32(&mut vm, "load", &[Value::I32(0)]),
        0x1234_5678
    );
}
