use wasm_parser::{decode_s33, parse_module, ParseError};
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::ValidationError;

const I32: u8 = 0x7f;

#[derive(Clone)]
struct TypeDef {
    params: Vec<u8>,
    results: Vec<u8>,
}

fn ty(params: &[u8], results: &[u8]) -> TypeDef {
    TypeDef {
        params: params.to_vec(),
        results: results.to_vec(),
    }
}

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

fn push_type(payload: &mut Vec<u8>, def: &TypeDef) {
    payload.push(0x60);
    push_u32(payload, def.params.len() as u32);
    payload.extend_from_slice(&def.params);
    push_u32(payload, def.results.len() as u32);
    payload.extend_from_slice(&def.results);
}

fn build_module(types: &[TypeDef], function_type: u32, instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    let mut type_section = Vec::new();
    push_u32(&mut type_section, types.len() as u32);
    for def in types {
        push_type(&mut type_section, def);
    }
    push_section(&mut module, 1, &type_section);

    let mut function_section = vec![0x01];
    push_u32(&mut function_section, function_type);
    push_section(&mut module, 3, &function_section);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);

    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code_section = vec![0x01];
    push_u32(&mut code_section, body.len() as u32);
    code_section.extend_from_slice(&body);
    push_section(&mut module, 10, &code_section);
    module
}

fn instance(bytes: &[u8]) -> Instance {
    Instance::new(parse_module(bytes).expect("parse Phase 5C fixture"))
        .expect("instantiate Phase 5C fixture")
}

#[test]
fn type_index_block_carries_parameter_and_result() {
    let module = build_module(
        &[ty(&[I32], &[I32])],
        0,
        &[0x20, 0x00, 0x02, 0x00, 0x41, 0x01, 0x6a, 0x0b],
    );
    assert_eq!(
        instance(&module)
            .invoke_export("run", &[Value::I32(41)])
            .unwrap(),
        Some(Value::I32(42))
    );
}

#[test]
fn loop_branch_uses_block_parameter_as_label_value() {
    let module = build_module(
        &[ty(&[I32], &[I32])],
        0,
        &[
            0x20, 0x00, // initial loop parameter
            0x03, 0x00, // loop (type 0: i32 -> i32)
            0x21, 0x00, // local.set 0
            0x20, 0x00, 0x41, 0x01, 0x6b, // local.get 0; i32.const 1; sub
            0x22, 0x00, // local.tee 0 leaves the next loop parameter
            0x20, 0x00, // condition
            0x0d, 0x00, // br_if 0 preserves the loop parameter
            0x0b,
        ],
    );
    assert_eq!(
        instance(&module)
            .invoke_export("run", &[Value::I32(3)])
            .unwrap(),
        Some(Value::I32(0))
    );
}

#[test]
fn if_else_restarts_each_arm_with_block_parameters() {
    let block_type = ty(&[I32], &[I32]);
    let function_type = ty(&[I32, I32], &[I32]);
    let module = build_module(
        &[block_type, function_type],
        1,
        &[
            0x20, 0x00, // block parameter
            0x20, 0x01, // condition
            0x04, 0x00, // if type 0
            0x41, 0x01, 0x6a, // then: param + 1
            0x05, // else: validator/runtime must restore the param
            0x41, 0x02, 0x6a, // else: param + 2
            0x0b,
        ],
    );
    let mut vm = instance(&module);
    assert_eq!(
        vm.invoke_export("run", &[Value::I32(10), Value::I32(1)])
            .unwrap(),
        Some(Value::I32(11))
    );
    assert_eq!(
        vm.invoke_export("run", &[Value::I32(10), Value::I32(0)])
            .unwrap(),
        Some(Value::I32(12))
    );
}

#[test]
fn branch_from_indexed_block_preserves_result_label_value() {
    let module = build_module(
        &[ty(&[I32], &[I32])],
        0,
        &[0x20, 0x00, 0x02, 0x00, 0x0c, 0x00, 0x0b],
    );
    assert_eq!(
        instance(&module)
            .invoke_export("run", &[Value::I32(77)])
            .unwrap(),
        Some(Value::I32(77))
    );
}

#[test]
fn multi_byte_type_index_block_is_decoded_as_s33() {
    let types = vec![ty(&[I32], &[I32]); 131];
    let module = build_module(
        &types,
        0,
        &[
            0x20, 0x00, 0x02, 0x82, 0x01, // block type index 130
            0x41, 0x01, 0x6a, 0x0b,
        ],
    );
    assert_eq!(
        instance(&module)
            .invoke_export("run", &[Value::I32(8)])
            .unwrap(),
        Some(Value::I32(9))
    );
}

#[test]
fn missing_block_type_index_is_rejected_before_execution() {
    let module = build_module(&[ty(&[], &[])], 0, &[0x02, 0x01, 0x0b]);
    let error = Instance::new(parse_module(&module).unwrap()).expect_err("missing type must fail");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::BlockTypeIndexOutOfBounds { type_index: 1, .. })
    ));
}

#[test]
fn multi_result_block_signature_executes_in_order() {
    let module = build_module(
        &[ty(&[], &[I32, I32]), ty(&[], &[I32, I32])],
        0,
        &[0x02, 0x01, 0x41, 0x07, 0x41, 0x09, 0x0b],
    );
    assert_eq!(
        instance(&module).invoke_export_values("run", &[]).unwrap(),
        vec![Value::I32(7), Value::I32(9)]
    );
}

#[test]
fn signed_33_decoder_covers_type_index_domain_and_rejects_overflow() {
    assert_eq!(decode_s33(&[0x00]).unwrap(), (0, 1));
    assert_eq!(decode_s33(&[0x7f]).unwrap(), (-1, 1));
    assert_eq!(
        decode_s33(&[0xff, 0xff, 0xff, 0xff, 0x0f]).unwrap(),
        (u32::MAX as i64, 5)
    );
    assert_eq!(
        decode_s33(&[0x80, 0x80, 0x80, 0x80, 0x70]).unwrap(),
        (-(1i64 << 32), 5)
    );
    assert_eq!(
        decode_s33(&[0xff, 0xff, 0xff, 0xff, 0x1f]),
        Err(ParseError::Leb128Overflow)
    );
}
