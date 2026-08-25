use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError};
use wasm_validator::ValidationError;

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

fn header() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

#[test]
fn upstream_active_element_without_table_is_rejected_during_validation() {
    // WebAssembly/spec test/core/elem.wast: a legacy active segment implicitly
    // targets table 0, which must exist.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let mut module = header();
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x00]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(
        &mut module,
        9,
        &[
            0x01, // one element segment
            0x00, // legacy active mode, table 0
            0x41, 0x00, 0x0b, // i32.const 0; end
            0x01, 0x00, // function vector [0]
        ],
    );
    push_section(&mut module, 10, &[0x01, 0x02, 0x00, 0x0b]);

    let parsed = parse_module(&module).expect("element-without-table vector must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::ElementTableOutOfBounds {
                segment: 0,
                table_index: 0,
            }
        ))
    ));
}

#[test]
fn active_element_function_index_must_exist_before_instantiation() {
    let mut module = header();
    push_section(&mut module, 4, &[0x01, 0x70, 0x00, 0x01]);
    push_section(
        &mut module,
        9,
        &[
            0x01, // one element segment
            0x00, // legacy active mode, table 0
            0x41, 0x00, 0x0b, // i32.const 0; end
            0x01, 0x00, // missing function index 0
        ],
    );

    let parsed = parse_module(&module).expect("bad element function-index vector must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::ElementFunctionOutOfBounds {
                segment: 0,
                function_index: 0,
            }
        ))
    ));
}

#[test]
fn upstream_active_data_without_memory_is_rejected_during_validation() {
    // WebAssembly/spec test/core/data.wast: a legacy active data segment
    // implicitly targets memory 0, which must exist even for an empty payload.
    let mut module = header();
    push_section(
        &mut module,
        11,
        &[
            0x01, // one data segment
            0x00, // legacy active mode, memory 0
            0x41, 0x00, 0x0b, // i32.const 0; end
            0x00, // empty byte vector
        ],
    );

    let parsed = parse_module(&module).expect("data-without-memory vector must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::DataMemoryOutOfBounds {
                segment: 0,
                memory_index: 0,
            }
        ))
    ));
}
