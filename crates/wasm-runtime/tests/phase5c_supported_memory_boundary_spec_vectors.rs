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

fn push_sleb(bytes: &mut Vec<u8>, mut value: i64) {
    loop {
        let mut byte = (value as u8) & 0x7f;
        let sign_bit_set = byte & 0x40 != 0;
        value >>= 7;
        let done = (value == 0 && !sign_bit_set) || (value == -1 && sign_bit_set);
        if !done {
            byte |= 0x80;
        }
        bytes.push(byte);
        if done {
            break;
        }
    }
}

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn push_i32_const(instructions: &mut Vec<u8>, value: i32) {
    instructions.push(0x41);
    push_sleb(instructions, value as i64);
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

fn roundtrip_at(address: i32, store: u8, load: u8, alignment: u8) -> Vec<u8> {
    let mut instructions = Vec::new();
    push_i32_const(&mut instructions, address);
    instructions.extend([0x20, 0x00, store, alignment, 0x00]);
    push_i32_const(&mut instructions, address);
    instructions.extend([load, alignment, 0x00]);
    single_memory_function_module(&[I32], Some(I32), &instructions)
}

fn load_at(opcode: u8, alignment: u8) -> Vec<u8> {
    single_memory_function_module(
        &[I32],
        Some(I32),
        &[0x20, 0x00, opcode, alignment, 0x00],
    )
}

fn store_at(opcode: u8, alignment: u8) -> Vec<u8> {
    single_memory_function_module(
        &[I32, I32],
        None,
        &[0x20, 0x00, 0x20, 0x01, opcode, alignment, 0x00],
    )
}

fn instance(bytes: &[u8]) -> Instance {
    Instance::new(parse_module(bytes).expect("translated memory boundary vector must parse"))
        .expect("translated memory boundary vector must validate and instantiate")
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
fn upstream_last_valid_page_end_accesses_succeed() {
    // WebAssembly/spec test/core/memory_trap.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    for (address, store, load, alignment, input, expected) in [
        ((WASM_PAGE_SIZE - 1) as i32, 0x3a, 0x2d, 0x00, -1, 255),
        (
            (WASM_PAGE_SIZE - 2) as i32,
            0x3b,
            0x2f,
            0x01,
            -1,
            65_535,
        ),
        (
            (WASM_PAGE_SIZE - 4) as i32,
            0x36,
            0x28,
            0x02,
            0x1234_5678,
            0x1234_5678,
        ),
    ] {
        let module = roundtrip_at(address, store, load, alignment);
        let mut vm = instance(&module);
        assert_eq!(
            vm.invoke_export("run", &[Value::I32(input)])
                .expect("last valid page-end access must execute"),
            Some(Value::I32(expected))
        );
    }
}

#[test]
fn upstream_first_invalid_load_starts_trap_at_exact_width() {
    // Width-sensitive first-invalid starts from memory_trap.wast.
    for (opcode, alignment, address, width) in [
        (0x2d, 0x00, WASM_PAGE_SIZE, 1usize),
        (0x2f, 0x01, WASM_PAGE_SIZE - 1, 2usize),
        (0x28, 0x02, WASM_PAGE_SIZE - 3, 4usize),
    ] {
        let module = load_at(opcode, alignment);
        let mut vm = instance(&module);
        let error = vm
            .invoke_export("run", &[Value::I32(address as i32)])
            .expect_err("first invalid load start must trap");
        assert_memory_oob(error, address as u64, width);
    }
}

#[test]
fn failed_page_end_stores_are_atomic() {
    for (opcode, alignment, address, width) in [
        (0x3a, 0x00, WASM_PAGE_SIZE, 1usize),
        (0x3b, 0x01, WASM_PAGE_SIZE - 1, 2usize),
        (0x36, 0x02, WASM_PAGE_SIZE - 3, 4usize),
    ] {
        let module = store_at(opcode, alignment);
        let mut vm = instance(&module);
        let tail_before = vm.memory().expect("defined memory").bytes()[WASM_PAGE_SIZE - 4..].to_vec();
        let error = vm
            .invoke_export(
                "run",
                &[Value::I32(address as i32), Value::I32(0x1234_5678)],
            )
            .expect_err("out-of-bounds store must trap before mutation");
        assert_memory_oob(error, address as u64, width);
        assert_eq!(
            &vm.memory().expect("defined memory").bytes()[WASM_PAGE_SIZE - 4..],
            tail_before.as_slice(),
            "failed store must not partially modify the in-bounds prefix"
        );
    }
}

#[test]
fn high_bit_i32_address_is_unsigned_and_never_host_signed_indexed() {
    // memory_trap.wast explicitly probes 0x80000000 as a dynamic address.
    let module = load_at(0x28, 0x02);
    let mut vm = instance(&module);
    let error = vm
        .invoke_export("run", &[Value::I32(i32::MIN)])
        .expect_err("0x80000000 must be interpreted as an unsigned i32 address and trap");
    assert_memory_oob(error, 0x8000_0000, 4);
}
