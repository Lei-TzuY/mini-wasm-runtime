use wasm_parser::{parse_module, ParseError};
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

fn minimal_function_module(result_type: Option<u8>, instructions: &[u8]) -> Vec<u8> {
    let mut module = header();

    let mut type_section = vec![0x01, 0x60, 0x00];
    match result_type {
        Some(value_type) => type_section.extend_from_slice(&[0x01, value_type]),
        None => type_section.push(0x00),
    }
    push_section(&mut module, 1, &type_section);
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
fn duplicate_standard_section_is_rejected_by_parser() {
    let mut module = header();
    push_section(&mut module, 1, &[0x00]);
    push_section(&mut module, 1, &[0x00]);

    assert_eq!(parse_module(&module), Err(ParseError::DuplicateSection(1)));
}

#[test]
fn standard_sections_must_remain_in_canonical_order() {
    let mut module = header();
    push_section(&mut module, 3, &[0x00]);
    push_section(&mut module, 1, &[0x00]);

    assert_eq!(
        parse_module(&module),
        Err(ParseError::SectionOutOfOrder {
            previous: 3,
            current: 1,
        })
    );
}

#[test]
fn missing_code_body_is_rejected_before_instantiation() {
    let mut module = header();
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x00]);
    push_section(&mut module, 3, &[0x01, 0x00]);

    let error = Instance::new(parse_module(&module).unwrap())
        .expect_err("function/code cardinality mismatch must fail closed");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::FunctionCodeLengthMismatch {
            functions: 1,
            bodies: 0,
        })
    ));
}

#[test]
fn duplicate_export_names_are_rejected_before_execution() {
    let mut module = minimal_function_module(None, &[]);

    let export_section = [
        0x02, // two exports
        0x01, b'x', 0x00, 0x00, // x -> function 0
        0x01, b'x', 0x00, 0x00, // duplicate x -> function 0
    ];
    // Export section must precede code, so rebuild with it inserted before code.
    module.truncate(14);
    push_section(&mut module, 7, &export_section);
    push_section(&mut module, 10, &[0x01, 0x02, 0x00, 0x0b]);

    let error = Instance::new(parse_module(&module).unwrap())
        .expect_err("duplicate export names must fail closed");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::DuplicateExportName(ref name)) if name == "x"
    ));
}

#[test]
fn out_of_bounds_start_function_is_rejected_before_execution() {
    let mut module = header();
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x00]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 8, &[0x01]);
    push_section(&mut module, 10, &[0x01, 0x02, 0x00, 0x0b]);

    let error = Instance::new(parse_module(&module).unwrap())
        .expect_err("missing start target must fail closed");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::StartFunctionOutOfBounds {
            function_index: 1,
        })
    ));
}

#[test]
fn memory_instruction_without_memory_is_rejected_before_execution() {
    let module = minimal_function_module(
        Some(0x7f),
        &[
            0x41, 0x00, // i32.const 0
            0x28, 0x02, 0x00, // i32.load align=4 offset=0
        ],
    );

    let error = Instance::new(parse_module(&module).unwrap())
        .expect_err("memory instruction without linear memory must fail closed");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::MemoryInstructionWithoutMemory { .. })
    ));
}
