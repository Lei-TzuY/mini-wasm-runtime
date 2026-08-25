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

fn function_module(params: &[u8], instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    let mut ty = vec![0x01, 0x60];
    push_u32(&mut ty, params.len() as u32);
    ty.extend_from_slice(params);
    ty.extend([0x01, I32]);
    push_section(&mut module, 1, &ty);
    push_section(&mut module, 3, &[0x01, 0x00]);
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

fn invoke_i32(module: &[u8], args: &[Value]) -> i32 {
    let module = parse_module(module).expect("translated control vector must parse");
    let mut instance =
        Instance::new(module).expect("translated supported control vector must validate");
    match instance
        .invoke_export("run", args)
        .expect("translated supported control vector must execute")
    {
        Some(Value::I32(value)) => value,
        other => panic!("translated control vector returned wrong value: {other:?}"),
    }
}

#[test]
fn upstream_br_if_result_value_survives_not_taken_path() {
    // Derived from WebAssembly/spec test/core/func.wast `break-br_if-num` at the
    // pinned revision. `drop` is outside this runtime's current opcode subset,
    // so the fallthrough path consumes the preserved label value with i32.add.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let mut instructions = vec![0x02, I32]; // block (result i32)
    push_i32_const(&mut instructions, 50); // candidate branch result
    instructions.extend([0x20, 0x00, 0x0d, 0x00]); // local.get 0; br_if 0
    push_i32_const(&mut instructions, 1);
    instructions.extend([0x6a, 0x0b]); // fallthrough: preserved 50 + 1; end block

    let module = function_module(&[I32], &instructions);
    assert_eq!(invoke_i32(&module, &[Value::I32(1)]), 50);
    assert_eq!(invoke_i32(&module, &[Value::I32(0)]), 51);
}

#[test]
fn nested_branch_carries_target_result_and_skips_polymorphic_tail() {
    // Exercises the same branch-result rule used by the `break-*` vectors in
    // WebAssembly/spec test/core/func.wast, but across one nested control frame.
    let mut instructions = vec![
        0x02, I32, // outer block (result i32)
        0x02, 0x40, // inner block with no result
    ];
    push_i32_const(&mut instructions, 79);
    instructions.extend([
        0x0c, 0x01, // br 1: exit outer block carrying 79
        0x6a, // unreachable i32.add: valid under stack-polymorphic typing
        0x0b, // end inner block
    ]);
    push_i32_const(&mut instructions, 0); // statically valid fallthrough result
    instructions.push(0x0b); // end outer block

    let module = function_module(&[], &instructions);
    assert_eq!(invoke_i32(&module, &[]), 79);
}

#[test]
fn return_value_skips_stack_polymorphic_tail() {
    // Strengthens the pinned `return-i32` vector by retaining a supported
    // instruction after return; validation must accept the unreachable tail.
    let mut instructions = Vec::new();
    push_i32_const(&mut instructions, 78);
    instructions.extend([
        0x0f, // return 78
        0x6a, // unreachable i32.add with no concrete operands
    ]);

    let module = function_module(&[], &instructions);
    assert_eq!(invoke_i32(&module, &[]), 78);
}
