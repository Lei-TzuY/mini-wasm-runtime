use wasm_parser::{parse_module, ParseError, ValueType};

const UPSTREAM_SPEC_COMMIT: &str = "fc209c5ed8afc4dfeb9252024d217da3376c7a6f";

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

fn module_with_imported_i32_global() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(
        &mut module,
        2,
        &[
            0x01, // one import
            0x03, b'e', b'n', b'v', // module "env"
            0x01, b'g', // name "g"
            0x03, // global import
            0x7f, 0x00, // immutable i32
        ],
    );
    module
}

fn active_data_with_offset_expr(expr: &[u8]) -> Vec<u8> {
    let mut module = module_with_imported_i32_global();
    push_section(&mut module, 5, &[0x01, 0x00, 0x01]); // memory 1

    let mut data = vec![0x01, 0x00]; // one legacy active segment
    data.extend_from_slice(expr);
    data.push(0x00); // empty byte vector
    push_section(&mut module, 11, &data);
    module
}

fn active_element_with_offset_expr(expr: &[u8]) -> Vec<u8> {
    let mut module = module_with_imported_i32_global();
    push_section(&mut module, 4, &[0x01, 0x70, 0x00, 0x01]); // table 1 funcref

    let mut elements = vec![0x01, 0x00]; // one legacy active segment
    elements.extend_from_slice(expr);
    elements.push(0x00); // empty function-index vector
    push_section(&mut module, 9, &elements);
    module
}

fn assert_both_segment_offsets_fail(expr: &[u8], expected: ParseError) {
    for (kind, module) in [
        ("data", active_data_with_offset_expr(expr)),
        ("element", active_element_with_offset_expr(expr)),
    ] {
        assert_eq!(
            parse_module(&module),
            Err(expected.clone()),
            "unexpected parser result for {kind} offset"
        );
    }
}

#[test]
fn upstream_segment_offsets_reject_non_i32_numeric_constants() {
    // WebAssembly/spec data.wast and elem.wast both require active offsets to
    // have i32 type. The current literal-only const-expr subset must preserve
    // that rule before instantiation.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    assert_both_segment_offsets_fail(
        &[0x42, 0x00, 0x0b], // i64.const 0; end
        ParseError::ConstExprTypeMismatch {
            expected: ValueType::I32,
            actual: ValueType::I64,
        },
    );
}

#[test]
fn upstream_imported_global_segment_offsets_remain_fail_closed() {
    // MVP constant expressions allow global.get of an imported immutable
    // global. Phase 5C intentionally keeps active offsets literal-only until
    // parser, validator, and instantiation semantics for global.get const-exprs
    // are implemented together. Both segment paths must reject at the opcode.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    assert_both_segment_offsets_fail(
        &[0x23, 0x00, 0x0b], // global.get 0; end
        ParseError::InvalidConstExprOpcode(0x23),
    );
}

#[test]
fn upstream_segment_offsets_reject_reference_and_empty_expressions() {
    assert_both_segment_offsets_fail(
        &[0xd0, 0x70, 0x0b], // ref.null func; end
        ParseError::InvalidConstExprOpcode(0xd0),
    );
    assert_both_segment_offsets_fail(
        &[0x0b], // empty instruction sequence; end
        ParseError::InvalidConstExprOpcode(0x0b),
    );
}

#[test]
fn upstream_segment_offsets_reject_multiple_values_in_one_const_expr() {
    assert_both_segment_offsets_fail(
        &[
            0x41, 0x00, // i32.const 0
            0x41, 0x00, // i32.const 0 instead of end
            0x0b,
        ],
        ParseError::ConstExprMissingEnd,
    );
}
