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

#[test]
fn out_of_bounds_function_export_is_rejected_before_execution() {
    let mut module = header();
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x00]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(
        &mut module,
        7,
        &[
            0x01, // one export
            0x03, b'r', b'u', b'n', 0x00, 0x01, // run -> missing function 1
        ],
    );
    push_section(&mut module, 10, &[0x01, 0x02, 0x00, 0x0b]);

    let error = Instance::new(parse_module(&module).unwrap())
        .expect_err("out-of-bounds function export must fail closed");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::FunctionExportOutOfBounds {
            function_index: 1,
            ..
        })
    ));
}

#[test]
fn start_function_with_non_empty_signature_is_rejected_before_execution() {
    let mut module = header();
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x01, 0x7f]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 8, &[0x00]);
    push_section(
        &mut module,
        10,
        &[
            0x01, // one body
            0x04, // body size
            0x00, // no locals
            0x41, 0x00, // i32.const 0
            0x0b, // end
        ],
    );

    let error = Instance::new(parse_module(&module).unwrap())
        .expect_err("start function must have [] -> [] signature");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::InvalidStartSignature { function_index: 0 })
    ));
}

#[test]
fn element_segment_with_missing_function_is_rejected_before_execution() {
    let mut module = header();
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x00]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(
        &mut module,
        4,
        &[
            0x01, // one table
            0x70, // funcref
            0x00, 0x01, // min-only, min = 1
        ],
    );
    push_section(
        &mut module,
        9,
        &[
            0x01, // one element segment
            0x00, // active, implicit table 0
            0x41, 0x00, 0x0b, // i32.const 0; end
            0x01, // one function index
            0x01, // missing function 1
        ],
    );
    push_section(&mut module, 10, &[0x01, 0x02, 0x00, 0x0b]);

    let error = Instance::new(parse_module(&module).unwrap())
        .expect_err("element segment must not reference a missing function");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::ElementFunctionOutOfBounds {
            segment: 0,
            function_index: 1,
        })
    ));
}

#[test]
fn data_segment_without_memory_is_rejected_before_execution() {
    let mut module = header();
    push_section(
        &mut module,
        11,
        &[
            0x01, // one data segment
            0x00, // active, implicit memory 0
            0x41, 0x00, 0x0b, // i32.const 0; end
            0x01, b'x', // one payload byte
        ],
    );

    let error = Instance::new(parse_module(&module).unwrap())
        .expect_err("active data segment requires a declared memory");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::DataMemoryOutOfBounds {
            segment: 0,
            memory_index: 0,
        })
    ));
}
