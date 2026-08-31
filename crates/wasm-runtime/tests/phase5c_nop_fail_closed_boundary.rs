use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, RuntimeError, RuntimeLimits, Value};

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

fn module_with_signature(results: &[u8], instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let mut ty = vec![0x01, 0x60, 0x00, results.len() as u8];
    ty.extend_from_slice(results);
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

fn module_with_instructions(instructions: &[u8]) -> Vec<u8> {
    module_with_signature(&[], instructions)
}

fn module_with_i32_result(instructions: &[u8]) -> Vec<u8> {
    module_with_signature(&[0x7f], instructions)
}

fn invoke_no_result(instructions: &[u8]) {
    let module = parse_module(&module_with_instructions(instructions))
        .expect("nop fixture must remain parseable");
    let mut instance = Instance::new(module).expect("nop must validate and instantiate");
    assert_eq!(instance.invoke_export("run", &[]).unwrap(), None);
}

fn invoke_i32_result(instructions: &[u8]) -> i32 {
    let module = parse_module(&module_with_i32_result(instructions))
        .expect("result-producing nop fixture must remain parseable");
    let mut instance = Instance::new(module).expect("nop must validate and instantiate");
    match instance.invoke_export("run", &[]).unwrap() {
        Some(Value::I32(value)) => value,
        other => panic!("expected i32 result, got {other:?}"),
    }
}

#[test]
fn reachable_nop_executes_as_stack_neutral_noop() {
    invoke_no_result(&[0x01]);
}

#[test]
fn structured_control_admits_nop_without_changing_stack() {
    invoke_no_result(&[
        0x02, 0x40, // block
        0x01, // nop
        0x0b,
    ]);
}

#[test]
fn if_else_control_admits_nop_in_selected_arm() {
    invoke_no_result(&[
        0x41, 0x00, // i32.const 0
        0x04, 0x40, // if
        0x05, // else
        0x01, // nop
        0x0b,
    ]);
}

#[test]
fn validator_unreachable_frame_still_decodes_nop() {
    invoke_no_result(&[
        0x02, 0x40, // block
        0x0c, 0x00, // br 0 makes the remainder validator-unreachable
        0x01, // nop must remain legal in unreachable code
        0x0b,
    ]);
}

#[test]
fn function_level_validator_unreachable_still_decodes_nop() {
    invoke_no_result(&[
        0x0f, // return makes the remainder validator-unreachable
        0x01, // nop remains legal and control-map decodable
    ]);
}

#[test]
fn result_function_nop_preserves_result_value() {
    assert_eq!(
        invoke_i32_result(&[
            0x01, // nop
            0x41, 0x2a, // i32.const 42
        ]),
        42
    );
}

#[test]
fn result_block_nop_preserves_result_value() {
    assert_eq!(
        invoke_i32_result(&[
            0x02, 0x7f, // block (result i32)
            0x01, // nop
            0x41, 0x2a, // i32.const 42
            0x0b,
        ]),
        42
    );
}

#[test]
fn nop_consumes_normal_instruction_fuel() {
    let module = parse_module(&module_with_instructions(&[0x01]))
        .expect("fuel fixture must remain parseable");
    let limits = RuntimeLimits {
        fuel: Some(1),
        ..RuntimeLimits::default()
    };
    let mut instance = Instance::with_config(module, HostRegistry::new(), limits)
        .expect("nop must validate and instantiate");
    assert!(matches!(
        instance.invoke_export("run", &[]),
        Err(RuntimeError::FuelExhausted)
    ));
}
