use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError};
use wasm_validator::ValidationError;

const I32: u8 = 0x7f;
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

fn module_with_body(params: u8, results: u8, body: &[u8]) -> Vec<u8> {
    let mut type_section = vec![0x01, 0x60, params];
    type_section.extend(std::iter::repeat(I32).take(params as usize));
    type_section.push(results);
    type_section.extend(std::iter::repeat(I32).take(results as usize));

    let mut function_body = vec![0x00];
    function_body.extend_from_slice(body);
    let mut code_section = vec![0x01];
    push_u32(&mut code_section, function_body.len() as u32);
    code_section.extend_from_slice(&function_body);

    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut module, 1, &type_section);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 10, &code_section);
    module
}

fn assert_br_table_rejected(
    params: u8,
    results: u8,
    body: &[u8],
    expected_offset: usize,
) {
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);
    let module = parse_module(&module_with_body(params, results, body))
        .expect("pinned br_table vector must remain structurally parseable");
    let error = Instance::new(module)
        .expect_err("br_table must remain fail-closed until the full vertical slice lands");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::UnsupportedOpcode {
            function: 0,
            offset,
            opcode: 0x0e,
        }) if offset == expected_offset
    ));
}

#[test]
fn pinned_empty_value_shape_remains_fail_closed() {
    // Source shape: br_table 0 (i32.const 33) (local.get 0), from br_table.wast.
    // Keep the branch value on the operand stack so future admission must validate
    // label values rather than treating br_table as a selector-only branch.
    assert_br_table_rejected(
        1,
        1,
        &[
            0x02, I32, // block (result i32)
            0x41, 0x21, // i32.const 33 -- branch value
            0x20, 0x00, // local.get 0 -- selector
            0x0e, 0x00, 0x00, // br_table [] default 0
            0x41, 0x1f, // unreachable fallback i32.const 31
            0x0b, // end block
            0x0b, // end function
        ],
        6,
    );
}

#[test]
fn pinned_singleton_shape_remains_fail_closed() {
    // Source shape: br_table 1 0 (local.get 0), from br_table.wast.
    // This pins distinct explicit/default depths inside nested labels.
    assert_br_table_rejected(
        1,
        1,
        &[
            0x02, 0x40, // outer block
            0x02, 0x40, // inner block
            0x20, 0x00, // local.get 0 -- selector
            0x0e, 0x01, 0x01, 0x00, // targets [1], default 0
            0x41, 0x15, 0x0f, // unreachable return 21
            0x0b, // end inner
            0x41, 0x14, 0x0f, // return 20
            0x0b, // end outer
            0x41, 0x16, // i32.const 22
            0x0b, // end function
        ],
        6,
    );
}

#[test]
fn pinned_multiple_target_shape_remains_fail_closed() {
    // Source shape: br_table 3 2 1 0 4 (local.get 0), from br_table.wast.
    // A future decoder must consume the whole target vector plus default exactly.
    assert_br_table_rejected(
        1,
        1,
        &[
            0x02, 0x40, // label depth 4
            0x02, 0x40, // label depth 3
            0x02, 0x40, // label depth 2
            0x02, 0x40, // label depth 1
            0x02, 0x40, // label depth 0
            0x20, 0x00, // local.get 0 -- selector
            0x0e, 0x04, 0x03, 0x02, 0x01, 0x00, 0x04,
            0x41, 0x63, 0x0f, // unreachable return payload
            0x0b,
            0x41, 0x64, 0x0f, // unreachable return payload
            0x0b,
            0x41, 0x65, 0x0f, // unreachable return payload
            0x0b,
            0x41, 0x66, 0x0f, // unreachable return payload
            0x0b,
            0x41, 0x67, 0x0f, // unreachable return payload
            0x0b,
            0x41, 0x68, // final payload
            0x0b,
        ],
        12,
    );
}

#[test]
fn pinned_negative_selector_shape_remains_fail_closed() {
    // WebAssembly br_table interprets the i32 selector as u32. A future runtime
    // implementation must therefore send -1 to the default arm, never index from
    // the end or sign-extend into host usize arithmetic.
    assert_br_table_rejected(
        0,
        0,
        &[
            0x41, 0x7f, // i32.const -1
            0x0e, 0x01, 0x00, 0x00, // targets [0], default 0
            0x0b,
        ],
        2,
    );
}
