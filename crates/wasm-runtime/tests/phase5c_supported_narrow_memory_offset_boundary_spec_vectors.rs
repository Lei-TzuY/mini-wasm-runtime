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

fn push_i32(bytes: &mut Vec<u8>, mut value: i32) {
    loop {
        let byte = (value as u8) & 0x7f;
        let sign_bit_set = byte & 0x40 != 0;
        value >>= 7;
        let done = (value == 0 && !sign_bit_set) || (value == -1 && sign_bit_set);
        bytes.push(if done { byte } else { byte | 0x80 });
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

fn push_i32_const(instructions: &mut Vec<u8>, value: i32) {
    instructions.push(0x41);
    push_i32(instructions, value);
}

fn roundtrip_with_offset(
    base: i32,
    store_opcode: u8,
    load_opcode: u8,
    alignment: u8,
    offset: u32,
) -> Vec<u8> {
    let mut instructions = Vec::new();
    push_i32_const(&mut instructions, base);
    instructions.extend([0x20, 0x00, store_opcode, alignment]);
    push_u32(&mut instructions, offset);
    push_i32_const(&mut instructions, base);
    instructions.extend([load_opcode, alignment]);
    push_u32(&mut instructions, offset);
    single_memory_function_module(&[I32], Some(I32), &instructions)
}

fn load_with_offset(opcode: u8, alignment: u8, offset: u32) -> Vec<u8> {
    let mut instructions = vec![0x20, 0x00, opcode, alignment];
    push_u32(&mut instructions, offset);
    single_memory_function_module(&[I32], Some(I32), &instructions)
}

fn store_with_offset(opcode: u8, alignment: u8, offset: u32) -> Vec<u8> {
    let mut instructions = vec![0x20, 0x00, 0x20, 0x01, opcode, alignment];
    push_u32(&mut instructions, offset);
    single_memory_function_module(&[I32, I32], None, &instructions)
}

fn instance(bytes: &[u8]) -> Instance {
    Instance::new(parse_module(bytes).expect("narrow offset vector must parse"))
        .expect("narrow offset vector must validate and instantiate")
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
fn narrow_memarg_offsets_reach_the_last_legal_bytes() {
    // Width-sensitive page-end cases derived from WebAssembly/spec
    // test/core/memory_trap.wast at the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    for (base, store, load, alignment, offset, input, expected) in [
        ((WASM_PAGE_SIZE - 8) as i32, 0x3a, 0x2d, 0x00, 7, -1, 255),
        ((WASM_PAGE_SIZE - 9) as i32, 0x3b, 0x2f, 0x01, 7, -1, 65_535),
    ] {
        let module = roundtrip_with_offset(base, store, load, alignment, offset);
        let mut vm = instance(&module);
        assert_eq!(
            vm.invoke_export("run", &[Value::I32(input)])
                .expect("last legal narrow offset access must execute"),
            Some(Value::I32(expected))
        );
    }
}

#[test]
fn narrow_memarg_offsets_trap_at_the_first_invalid_effective_address() {
    for (opcode, alignment, base, offset, effective, width) in [
        (0x2d, 0x00, WASM_PAGE_SIZE - 7, 7, WASM_PAGE_SIZE, 1usize),
        (
            0x2f,
            0x01,
            WASM_PAGE_SIZE - 8,
            7,
            WASM_PAGE_SIZE - 1,
            2usize,
        ),
    ] {
        let module = load_with_offset(opcode, alignment, offset);
        let mut vm = instance(&module);
        let error = vm
            .invoke_export("run", &[Value::I32(base as i32)])
            .expect_err("first invalid narrow offset load must trap");
        assert_memory_oob(error, effective as u64, width);
    }
}

#[test]
fn failed_narrow_offset_stores_are_atomic() {
    for (opcode, alignment, base, offset, effective, width) in [
        (0x3a, 0x00, WASM_PAGE_SIZE - 7, 7, WASM_PAGE_SIZE, 1usize),
        (
            0x3b,
            0x01,
            WASM_PAGE_SIZE - 8,
            7,
            WASM_PAGE_SIZE - 1,
            2usize,
        ),
    ] {
        let module = store_with_offset(opcode, alignment, offset);
        let mut vm = instance(&module);
        let tail_before =
            vm.memory().expect("defined memory").bytes()[WASM_PAGE_SIZE - 4..].to_vec();
        let error = vm
            .invoke_export("run", &[Value::I32(base as i32), Value::I32(0x1234_5678)])
            .expect_err("failed narrow offset store must trap before mutation");
        assert_memory_oob(error, effective as u64, width);
        assert_eq!(
            &vm.memory().expect("defined memory").bytes()[WASM_PAGE_SIZE - 4..],
            tail_before.as_slice(),
            "failed narrow offset store must not partially mutate the page tail"
        );
    }
}

#[test]
fn narrow_memarg_effective_addresses_never_wrap_at_u32() {
    for (opcode, alignment, base, offset, width) in [
        (0x2d, 0x00, -1, 1, 1usize),
        (0x2f, 0x01, -1, 1, 2usize),
        (0x2d, 0x00, 1, u32::MAX, 1usize),
        (0x2f, 0x01, 1, u32::MAX, 2usize),
    ] {
        let module = load_with_offset(opcode, alignment, offset);
        let mut vm = instance(&module);
        let error = vm
            .invoke_export("run", &[Value::I32(base)])
            .expect_err("u32 effective-address overflow must not wrap into memory");
        assert_memory_oob(error, 0x1_0000_0000, width);
    }
}
