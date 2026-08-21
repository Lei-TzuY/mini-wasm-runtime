use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::{validate, ValidationError};

const I32: u8 = 0x7f;
const I64: u8 = 0x7e;

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

fn header() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

fn two_result_type_section() -> Vec<u8> {
    vec![0x01, 0x60, 0x00, 0x02, I32, I64]
}

fn two_result_export_module() -> Vec<u8> {
    let mut module = header();
    push_section(&mut module, 1, &two_result_type_section());
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);
    push_section(
        &mut module,
        10,
        &[0x01, 0x06, 0x00, 0x41, 0x07, 0x42, 0x09, 0x0b],
    );
    module
}

fn direct_call_module() -> Vec<u8> {
    let mut module = header();
    push_section(&mut module, 1, &two_result_type_section());
    push_section(&mut module, 3, &[0x02, 0x00, 0x00]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x01]);

    let mut code = vec![0x02];
    let callee = [0x00, 0x41, 0x0b, 0x42, 0x16, 0x0b];
    push_u32(&mut code, callee.len() as u32);
    code.extend_from_slice(&callee);
    let caller = [0x00, 0x10, 0x00, 0x0b];
    push_u32(&mut code, caller.len() as u32);
    code.extend_from_slice(&caller);
    push_section(&mut module, 10, &code);
    module
}

fn indirect_call_module() -> Vec<u8> {
    let mut module = header();
    push_section(&mut module, 1, &two_result_type_section());
    push_section(&mut module, 3, &[0x02, 0x00, 0x00]);
    push_section(&mut module, 4, &[0x01, 0x70, 0x00, 0x01]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x01]);
    push_section(&mut module, 9, &[0x01, 0x00, 0x41, 0x00, 0x0b, 0x01, 0x00]);

    let mut code = vec![0x02];
    let callee = [0x00, 0x41, 0x21, 0x42, 0x2c, 0x0b];
    push_u32(&mut code, callee.len() as u32);
    code.extend_from_slice(&callee);
    let caller = [0x00, 0x41, 0x00, 0x11, 0x00, 0x00, 0x0b];
    push_u32(&mut code, caller.len() as u32);
    code.extend_from_slice(&caller);
    push_section(&mut module, 10, &code);
    module
}

fn branching_block_module() -> Vec<u8> {
    let mut module = header();
    push_section(&mut module, 1, &two_result_type_section());
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);

    let body = [
        0x00, 0x02, 0x00, 0x41, 0x03, 0x42, 0x04, 0x0c, 0x00, 0x0b, 0x0b,
    ];
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn multi_result_if_module() -> Vec<u8> {
    let mut module = header();
    let types = [
        0x02, 0x60, 0x00, 0x02, I32, I64, 0x60, 0x01, I32, 0x02, I32, I64,
    ];
    push_section(&mut module, 1, &types);
    push_section(&mut module, 3, &[0x01, 0x01]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);

    let body = [
        0x00, 0x20, 0x00, 0x04, 0x00, 0x41, 0x0b, 0x42, 0x16, 0x05, 0x41, 0x21, 0x42, 0x2c, 0x0b,
        0x0b,
    ];
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn wrong_result_order_module() -> Vec<u8> {
    let mut module = header();
    push_section(&mut module, 1, &two_result_type_section());
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(
        &mut module,
        10,
        &[0x01, 0x06, 0x00, 0x42, 0x01, 0x41, 0x02, 0x0b],
    );
    module
}

fn multi_result_import_module() -> Vec<u8> {
    let mut module = header();
    push_section(&mut module, 1, &two_result_type_section());
    push_section(
        &mut module,
        2,
        &[
            0x01, 0x03, b'e', b'n', b'v', 0x05, b'm', b'u', b'l', b't', b'i', 0x00, 0x00,
        ],
    );
    module
}

fn instance(bytes: Vec<u8>) -> Instance {
    Instance::new(parse_module(&bytes).expect("multi-value fixture must parse"))
        .expect("multi-value fixture must validate and instantiate")
}

#[test]
fn exported_defined_function_returns_ordered_multi_values() {
    let mut vm = instance(two_result_export_module());
    assert_eq!(
        vm.invoke_export_values("run", &[]).unwrap(),
        vec![Value::I32(7), Value::I64(9)]
    );
}

#[test]
fn legacy_invoke_api_rejects_multi_value_before_execution() {
    let mut vm = instance(two_result_export_module());
    assert!(matches!(
        vm.invoke_export("run", &[]),
        Err(RuntimeError::MultiValueResultRequiresValuesApi { results: 2 })
    ));
}

#[test]
fn direct_call_propagates_all_results_in_stack_order() {
    let mut vm = instance(direct_call_module());
    assert_eq!(
        vm.invoke_export_values("run", &[]).unwrap(),
        vec![Value::I32(11), Value::I64(22)]
    );
}

#[test]
fn indirect_call_propagates_all_results_after_dynamic_type_check() {
    let mut vm = instance(indirect_call_module());
    assert_eq!(
        vm.invoke_export_values("run", &[]).unwrap(),
        vec![Value::I32(33), Value::I64(44)]
    );
}

#[test]
fn branch_preserves_multi_value_block_label_vector() {
    let mut vm = instance(branching_block_module());
    assert_eq!(
        vm.invoke_export_values("run", &[]).unwrap(),
        vec![Value::I32(3), Value::I64(4)]
    );
}

#[test]
fn if_else_validates_and_returns_each_multi_value_arm() {
    let mut vm = instance(multi_result_if_module());
    assert_eq!(
        vm.invoke_export_values("run", &[Value::I32(1)]).unwrap(),
        vec![Value::I32(11), Value::I64(22)]
    );
    assert_eq!(
        vm.invoke_export_values("run", &[Value::I32(0)]).unwrap(),
        vec![Value::I32(33), Value::I64(44)]
    );
}

#[test]
fn validator_rejects_wrong_multi_result_order() {
    let module = parse_module(&wrong_result_order_module()).unwrap();
    assert!(matches!(
        validate(&module),
        Err(ValidationError::TypeMismatch { .. })
    ));
}

#[test]
fn multi_result_host_imports_remain_fail_closed_at_host_abi_boundary() {
    let module = parse_module(&multi_result_import_module()).unwrap();
    assert!(matches!(
        validate(&module),
        Err(ValidationError::UnsupportedImportResultArity {
            import: 0,
            results: 2
        })
    ));
}
