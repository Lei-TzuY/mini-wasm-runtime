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

fn push_body(payload: &mut Vec<u8>, instructions: &[u8]) {
    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    push_u32(payload, body.len() as u32);
    payload.extend_from_slice(&body);
}

fn single_memory_function_module(instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(
        &mut module,
        1,
        &[0x01, 0x60, 0x01, I32, 0x01, I32],
    );
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 5, &[0x01, 0x00, 0x01]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);
    let mut code = vec![0x01];
    push_body(&mut code, instructions);
    push_section(&mut module, 10, &code);
    module
}

fn roundtrip_module(address: i32, store: u8, load: u8, alignment: u8) -> Vec<u8> {
    let mut instructions = Vec::new();
    push_i32_const(&mut instructions, address);
    instructions.extend([0x20, 0x00, store, alignment, 0x00]);
    push_i32_const(&mut instructions, address);
    instructions.extend([load, alignment, 0x00]);
    single_memory_function_module(&instructions)
}

fn size_and_grow_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(
        &mut module,
        1,
        &[
            0x02, // two types
            0x60, 0x00, 0x01, I32, // type 0: () -> i32
            0x60, 0x01, I32, 0x01, I32, // type 1: (i32) -> i32
        ],
    );
    push_section(&mut module, 3, &[0x02, 0x00, 0x01]);
    push_section(&mut module, 5, &[0x01, 0x01, 0x01, 0x03]);
    push_section(
        &mut module,
        7,
        &[
            0x02, // two exports
            0x04, b's', b'i', b'z', b'e', 0x00, 0x00,
            0x04, b'g', b'r', b'o', b'w', 0x00, 0x01,
        ],
    );

    let mut code = vec![0x02];
    push_body(&mut code, &[0x3f, 0x00]);
    push_body(&mut code, &[0x20, 0x00, 0x40, 0x00]);
    push_section(&mut module, 10, &code);
    module
}

fn instance(bytes: &[u8]) -> Instance {
    Instance::new(parse_module(bytes).expect("translated memory spec vector must parse"))
        .expect("translated memory spec vector must validate and instantiate")
}

fn invoke_i32(instance: &mut Instance, name: &str, args: &[Value]) -> i32 {
    match instance
        .invoke_export(name, args)
        .expect("translated memory spec vector must execute")
    {
        Some(Value::I32(value)) => value,
        other => panic!("translated memory spec vector returned wrong value: {other:?}"),
    }
}

#[test]
fn upstream_i32_narrow_load_vectors_truncate_then_extend() {
    // WebAssembly/spec test/core/memory.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    for (store, load, alignment, input, expected) in [
        (0x3a, 0x2c, 0x00, 0x3456_cdef, -17),
        (0x3a, 0x2d, 0x00, 0xfedc_6543u32 as i32, 0x43),
        (0x3b, 0x2e, 0x01, 0x3456_cdef, -12_817),
        (0x3b, 0x2f, 0x01, 0x3456_cdef, 0xcdef),
    ] {
        let module = roundtrip_module(8, store, load, alignment);
        assert_eq!(invoke_i32(&mut instance(&module), "run", &[Value::I32(input)]), expected);
    }
}

#[test]
fn unaligned_i32_access_with_natural_alignment_is_legal() {
    // Alignment is a validation hint; the effective address itself need not be aligned.
    let module = roundtrip_module(1, 0x36, 0x28, 0x02);
    assert_eq!(
        invoke_i32(
            &mut instance(&module),
            "run",
            &[Value::I32(0x1234_5678)],
        ),
        0x1234_5678
    );
}

#[test]
fn memarg_offsets_participate_in_the_effective_address() {
    let mut instructions = Vec::new();
    push_i32_const(&mut instructions, 1);
    instructions.extend([0x20, 0x00, 0x36, 0x02, 0x03]); // store at 1 + 3
    push_i32_const(&mut instructions, 2);
    instructions.extend([0x28, 0x02, 0x02]); // load from 2 + 2

    let module = single_memory_function_module(&instructions);
    assert_eq!(
        invoke_i32(
            &mut instance(&module),
            "run",
            &[Value::I32(0x1020_3040)],
        ),
        0x1020_3040
    );
}

#[test]
fn upstream_memory_size_and_grow_zero_preserve_state() {
    // WebAssembly/spec test/core/memory_grow.wast @ the pinned revision.
    let module = size_and_grow_module();
    let mut vm = instance(&module);

    assert_eq!(invoke_i32(&mut vm, "size", &[]), 1);
    assert_eq!(invoke_i32(&mut vm, "grow", &[Value::I32(0)]), 1);
    assert_eq!(invoke_i32(&mut vm, "size", &[]), 1);

    assert_eq!(invoke_i32(&mut vm, "grow", &[Value::I32(1)]), 1);
    assert_eq!(invoke_i32(&mut vm, "size", &[]), 2);
    assert_eq!(invoke_i32(&mut vm, "grow", &[Value::I32(0)]), 2);
    assert_eq!(invoke_i32(&mut vm, "size", &[]), 2);

    assert_eq!(invoke_i32(&mut vm, "grow", &[Value::I32(1)]), 2);
    assert_eq!(invoke_i32(&mut vm, "size", &[]), 3);
    assert_eq!(invoke_i32(&mut vm, "grow", &[Value::I32(1)]), -1);
    assert_eq!(invoke_i32(&mut vm, "size", &[]), 3);
}
