use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value, WASM_PAGE_SIZE};

const I32: u8 = 0x7f;

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

fn single_memory_function_module(
    params: &[u8],
    result: Option<u8>,
    instructions: &[u8],
) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    let mut ty = vec![0x01, 0x60];
    push_u32(&mut ty, params.len() as u32);
    ty.extend_from_slice(params);
    match result {
        Some(result) => ty.extend([0x01, result]),
        None => ty.push(0x00),
    }
    push_section(&mut module, 1, &ty);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 5, &[0x01, 0x00, 0x01]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);

    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn load_with_offset(offset: u32) -> Vec<u8> {
    let mut instructions = vec![0x20, 0x00, 0x28, 0x02];
    push_u32(&mut instructions, offset);
    single_memory_function_module(&[I32], Some(I32), &instructions)
}

fn store_with_offset(offset: u32) -> Vec<u8> {
    let mut instructions = vec![0x20, 0x00, 0x20, 0x01, 0x36, 0x02];
    push_u32(&mut instructions, offset);
    single_memory_function_module(&[I32, I32], None, &instructions)
}

fn instance(bytes: &[u8]) -> Instance {
    Instance::new(parse_module(bytes).expect("memory-offset boundary vector must parse"))
        .expect("memory-offset boundary vector must validate and instantiate")
}

fn assert_memory_oob(error: RuntimeError, expected_address: u64, expected_width: usize) {
    match error {
        RuntimeError::MemoryOutOfBounds { address, width } => {
            assert_eq!(address, expected_address);
            assert_eq!(width, expected_width);
        }
        other => panic!("expected precise memory out-of-bounds trap, got {other:?}"),
    }
}

#[test]
fn memarg_offset_crossing_page_end_traps_at_effective_address() {
    let module = load_with_offset(1);
    let mut vm = instance(&module);
    let error = vm
        .invoke_export("run", &[Value::I32((WASM_PAGE_SIZE - 1) as i32)])
        .expect_err("base plus offset at the page boundary must trap");
    assert_memory_oob(error, WASM_PAGE_SIZE as u64, 4);
}

#[test]
fn memarg_offset_does_not_wrap_the_unsigned_i32_base() {
    let module = load_with_offset(1);
    let mut vm = instance(&module);
    let error = vm
        .invoke_export("run", &[Value::I32(-1)])
        .expect_err("0xffffffff + offset 1 must not wrap back to address zero");
    assert_memory_oob(error, 0x1_0000_0000, 4);
}

#[test]
fn failed_offset_store_is_atomic() {
    let module = store_with_offset(2);
    let mut vm = instance(&module);
    let tail_before = vm.memory().expect("defined memory").bytes()[WASM_PAGE_SIZE - 4..].to_vec();
    let base = (WASM_PAGE_SIZE - 3) as i32;
    let error = vm
        .invoke_export("run", &[Value::I32(base), Value::I32(0x1234_5678)])
        .expect_err("offset store crossing the page boundary must trap before mutation");
    assert_memory_oob(error, (WASM_PAGE_SIZE - 1) as u64, 4);
    assert_eq!(
        &vm.memory().expect("defined memory").bytes()[WASM_PAGE_SIZE - 4..],
        tail_before.as_slice(),
        "failed offset store must not partially modify the in-bounds prefix"
    );
}
