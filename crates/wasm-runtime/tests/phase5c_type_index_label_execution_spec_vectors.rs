use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};

const I32: u8 = 0x7f;
const I64: u8 = 0x7e;
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

fn module_with_types(
    function_result: u8,
    block_params: &[u8],
    block_result: u8,
    body: &[u8],
) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    let mut types = vec![
        0x02, // two types
        0x60,
        0x00,
        0x01,
        function_result, // type 0: () -> function_result
        0x60,
    ];
    push_u32(&mut types, block_params.len() as u32);
    types.extend_from_slice(block_params);
    types.extend([0x01, block_result]);
    push_section(&mut module, 1, &types);

    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);

    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(body);
    push_section(&mut module, 10, &code);
    module
}

fn invoke(module: &[u8]) -> Value {
    let module = parse_module(module).expect("type-index control vector must parse");
    let mut instance = Instance::new(module).expect("type-index control vector must validate");
    instance
        .invoke_export("run", &[])
        .expect("type-index control vector must execute")
        .expect("fixture declares one result")
}

#[test]
fn type_index_loop_br_if_uses_parameter_label_and_preserves_false_path_value() {
    // WebAssembly control typing gives loops their parameter types as label
    // types, unlike blocks/ifs which branch with their result types. This
    // countdown repeatedly branches with the updated loop parameter and then
    // falls through with zero as the loop result.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let body = [
        0x01, 0x01, I32, // one local group: one i32 local
        0x41, 0x03, // i32.const 3: initial loop parameter
        0x03, 0x01, // loop type 1: (param i32) (result i32)
        0x22, 0x00, // local.tee 0: retain current counter
        0x41, 0x01, // i32.const 1
        0x6b, // i32.sub
        0x22, 0x00, // local.tee 0: branch value = decremented counter
        0x20, 0x00, // local.get 0: condition
        0x0d, 0x00, // br_if 0: loop label consumes i32 parameter when taken
        0x0b, // end loop: false path retains the i32 branch value as result
        0x0b, // end function
    ];
    let module = module_with_types(I32, &[I32], I32, &body);

    assert_eq!(invoke(&module), Value::I32(0));
}

#[test]
fn type_index_block_branch_uses_result_label_not_parameter_label() {
    // A type-index block may have both parameters and a result. `br 0` must
    // carry the result type (i64 here) and discard the consumed i32 parameter;
    // treating a block label like a loop label would reject or mis-execute it.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let body = [
        0x00, // no locals
        0x41, 0xfb, 0x00, // i32.const 123: block parameter (signed LEB128)
        0x02, 0x01, // block type 1: (param i32) (result i64)
        0x42, 0xcd, 0x00, // i64.const 77 (signed LEB128)
        0x0c, 0x00, // br 0 carrying i64 77
        0x7c, // unreachable i64.add: validates stack-polymorphically
        0x0b, // end block
        0x0b, // end function
    ];
    let module = module_with_types(I64, &[I32], I64, &body);

    assert_eq!(invoke(&module), Value::I64(77));
}

#[test]
fn type_index_if_branch_uses_result_label_and_else_retains_parameter() {
    // Like blocks, if labels use the result type rather than the parameter
    // type. The taken arm branches with i64 77 and must discard the i32 block
    // parameter. The false arm receives that parameter and converts it to the
    // declared i64 result, pinning both branch-label and else-entry semantics.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let taken_body = [
        0x00, // no locals
        0x41, 0xfb, 0x00, // i32.const 123: if parameter (signed LEB128)
        0x41, 0x01, // i32.const 1: condition
        0x04, 0x01, // if type 1: (param i32) (result i64)
        0x42, 0xcd, 0x00, // i64.const 77 (signed LEB128)
        0x0c, 0x00, // br 0 carrying the i64 if-label result
        0x7c, // unreachable i64.add: stack-polymorphic tail
        0x05, // else
        0xac, // i64.extend_i32_s: consume restored i32 parameter
        0x0b, // end if
        0x0b, // end function
    ];
    let taken = module_with_types(I64, &[I32], I64, &taken_body);
    assert_eq!(invoke(&taken), Value::I64(77));

    let false_body = [
        0x00, // no locals
        0x41, 0xfb, 0x00, // i32.const 123: if parameter (signed LEB128)
        0x41, 0x00, // i32.const 0: condition
        0x04, 0x01, // if type 1: (param i32) (result i64)
        0x42, 0xcd, 0x00, // i64.const 77
        0x0c, 0x00, // branch is not executed on this path
        0x7c, // unreachable in the then arm only
        0x05, // else
        0xac, // consume else-entry i32 parameter as i64 result
        0x0b, // end if
        0x0b, // end function
    ];
    let not_taken = module_with_types(I64, &[I32], I64, &false_body);
    assert_eq!(invoke(&not_taken), Value::I64(123));
}
