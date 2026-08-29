use wasm_parser::parse_module;
use wasm_validator::{validate, ValidationError};

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    assert!(payload.len() < 128);
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

fn module_header() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

fn multivalue_type_section() -> [u8; 6] {
    [0x01, 0x60, 0x00, 0x02, 0x7f, 0x7e]
}

#[test]
fn unused_multivalue_type_declaration_does_not_expand_execution_admission() {
    let mut bytes = module_header();
    push_section(&mut bytes, 1, &multivalue_type_section());

    let module =
        parse_module(&bytes).expect("multi-result function types are valid module syntax");
    validate(&module).expect("an unused multi-result type does not require execution support");
}

#[test]
fn defined_function_using_multivalue_result_type_remains_fail_closed() {
    let mut bytes = module_header();
    push_section(&mut bytes, 1, &multivalue_type_section());
    push_section(&mut bytes, 3, &[0x01, 0x00]);
    push_section(&mut bytes, 10, &[0x01, 0x02, 0x00, 0x0b]);

    let module =
        parse_module(&bytes).expect("multi-result function signature must remain parseable");
    assert_eq!(
        validate(&module),
        Err(ValidationError::UnsupportedResultArity {
            function: 0,
            results: 2,
        })
    );
}
