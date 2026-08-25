use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError};
use wasm_validator::ValidationError;

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

fn minimal_function_module(instructions: &[u8]) -> Vec<u8> {
    let mut module = header();
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x00]);
    push_section(&mut module, 3, &[0x01, 0x00]);

    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code_section = vec![0x01];
    push_u32(&mut code_section, body.len() as u32);
    code_section.extend_from_slice(&body);
    push_section(&mut module, 10, &code_section);
    module
}

#[test]
fn function_import_type_index_must_exist() {
    let mut module = header();
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x00]);
    push_section(
        &mut module,
        2,
        &[
            0x01, // one import
            0x01, b'm', // module name
            0x01, b'f', // field name
            0x00, // function import
            0x01, // missing type index 1
        ],
    );

    let error = Instance::new(parse_module(&module).unwrap())
        .expect_err("function import must not refer to a missing type");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::ImportTypeIndexOutOfBounds {
            import: 0,
            type_index: 1,
        })
    ));
}

#[test]
fn defined_function_type_index_must_exist() {
    let mut module = header();
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x00]);
    push_section(&mut module, 3, &[0x01, 0x01]);
    push_section(&mut module, 10, &[0x01, 0x02, 0x00, 0x0b]);

    let error = Instance::new(parse_module(&module).unwrap())
        .expect_err("defined function must not refer to a missing type");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::TypeIndexOutOfBounds {
            function: 0,
            type_index: 1,
        })
    ));
}

#[test]
fn local_instruction_must_stay_inside_local_index_space() {
    let module = minimal_function_module(&[
        0x20, 0x00, // local.get 0 in a function with no params or locals
        0x1a, // drop, so the body would otherwise balance
    ]);

    let error = Instance::new(parse_module(&module).unwrap())
        .expect_err("local.get must not escape the local index space");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::LocalIndexOutOfBounds {
            function: 0,
            local_index: 0,
            ..
        })
    ));
}

#[test]
fn direct_call_target_must_exist() {
    let module = minimal_function_module(&[
        0x10, 0x01, // call missing function 1
    ]);

    let error = Instance::new(parse_module(&module).unwrap())
        .expect_err("direct call must not target a missing function");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::CallTargetOutOfBounds {
            function: 0,
            target: 1,
            ..
        })
    ));
}

#[test]
fn branch_depth_must_resolve_to_an_active_label() {
    let module = minimal_function_module(&[
        0x0c, 0x01, // br 1: only the function label at depth 0 exists
    ]);

    let error = Instance::new(parse_module(&module).unwrap())
        .expect_err("branch depth must resolve to an active control label");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::BranchDepthOutOfBounds {
            function: 0,
            depth: 1,
            ..
        })
    ));
}
