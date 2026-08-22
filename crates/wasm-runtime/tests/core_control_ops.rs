use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::ValidationError;

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    assert!(
        payload.len() < 128,
        "test helper only encodes one-byte lengths"
    );
    module.push(id);
    module.push(payload.len() as u8);
    module.extend(payload);
}

fn module_with_body(params: u8, results: u8, body: &[u8]) -> Vec<u8> {
    let mut type_section = vec![0x01, 0x60, params];
    type_section.extend(std::iter::repeat(0x7f).take(params as usize));
    type_section.push(results);
    type_section.extend(std::iter::repeat(0x7f).take(results as usize));
    let mut code_payload = vec![0x01, (body.len() + 1) as u8, 0x00];
    code_payload.extend(body);
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut bytes, 1, &type_section);
    push_section(&mut bytes, 3, &[0x01, 0x00]);
    push_section(&mut bytes, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);
    push_section(&mut bytes, 10, &code_payload);
    bytes
}

fn instance(bytes: &[u8]) -> Instance {
    Instance::new(parse_module(bytes).expect("parse test module")).expect("validate test module")
}

#[test]
fn nop_and_drop_execute() {
    let bytes = module_with_body(0, 1, &[0x01, 0x41, 0x07, 0x1a, 0x41, 0x2a, 0x0b]);
    let mut vm = instance(&bytes);
    assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(42)));
}

#[test]
fn select_chooses_first_for_nonzero_condition() {
    let bytes = module_with_body(0, 1, &[0x41, 0x0a, 0x41, 0x14, 0x41, 0x01, 0x1b, 0x0b]);
    let mut vm = instance(&bytes);
    assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(10)));
}

#[test]
fn select_chooses_second_for_zero_condition() {
    let bytes = module_with_body(0, 1, &[0x41, 0x0a, 0x41, 0x14, 0x41, 0x00, 0x1b, 0x0b]);
    let mut vm = instance(&bytes);
    assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(20)));
}

#[test]
fn select_rejects_mismatched_value_types() {
    let bytes = module_with_body(0, 1, &[0x41, 0x01, 0x42, 0x02, 0x41, 0x01, 0x1b, 0x0b]);
    let module = parse_module(&bytes).expect("parse test module");
    let error = Instance::new(module).expect_err("mismatched select types must fail");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::TypeMismatch { .. })
    ));
}

#[test]
fn br_table_chooses_index_or_default_target() {
    let bytes = module_with_body(
        1,
        1,
        &[
            0x02, 0x7f, 0x02, 0x7f, 0x41, 0x28, 0x20, 0x00, 0x0e, 0x01, 0x00, 0x01, 0x0b, 0x41,
            0x02, 0x6a, 0x0b, 0x0b,
        ],
    );
    let mut vm = instance(&bytes);
    assert_eq!(
        vm.invoke_export("run", &[Value::I32(0)]).unwrap(),
        Some(Value::I32(42))
    );
    assert_eq!(
        vm.invoke_export("run", &[Value::I32(1)]).unwrap(),
        Some(Value::I32(40))
    );
    assert_eq!(
        vm.invoke_export("run", &[Value::I32(i32::MAX)]).unwrap(),
        Some(Value::I32(40))
    );
}

#[test]
fn br_table_rejects_mixed_label_signatures() {
    let bytes = module_with_body(
        1,
        1,
        &[
            0x02, 0x7f, 0x02, 0x40, 0x41, 0x28, 0x20, 0x00, 0x0e, 0x01, 0x00, 0x01, 0x0b, 0x41,
            0x02, 0x0b, 0x0b,
        ],
    );
    let module = parse_module(&bytes).expect("parse test module");
    let error = Instance::new(module).expect_err("mixed br_table label types must fail");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::BranchTableTypeMismatch { .. })
    ));
}
