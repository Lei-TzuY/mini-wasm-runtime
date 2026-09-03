use wasm_parser::{parse_module, ParseError};

// Curated from WebAssembly/spec@fc209c5ed8afc4dfeb9252024d217da3376c7a6f
// test/core/binary.wast. Function and code section cardinality is a binary-format
// invariant owned by the parser; the validator keeps its duplicate check as
// defense in depth for programmatically constructed Module values.

fn bytes(hex: &str) -> Vec<u8> {
    hex.split_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).expect("test hex byte"))
        .collect()
}

fn module(tail: &str) -> Vec<u8> {
    let mut module = bytes("00 61 73 6d 01 00 00 00");
    module.extend(bytes(tail));
    module
}

#[test]
fn rejects_pinned_function_and_code_section_count_mismatches() {
    let cases = [
        (
            "function section present, code section absent",
            "01 04 01 60 00 00 03 03 02 00 00",
            2usize,
            0usize,
        ),
        (
            "code section present, function section absent",
            "0a 04 01 02 00 0b",
            0,
            1,
        ),
        (
            "function section count exceeds code section count",
            "01 04 01 60 00 00 03 03 02 00 00 0a 04 01 02 00 0b",
            2,
            1,
        ),
        (
            "code section count exceeds function section count",
            "01 04 01 60 00 00 03 02 01 00 0a 07 02 02 00 0b 02 00 0b",
            1,
            2,
        ),
    ];

    for (name, tail, functions, bodies) in cases {
        assert_eq!(
            parse_module(&module(tail)),
            Err(ParseError::FunctionCodeLengthMismatch { functions, bodies }),
            "pinned binary.wast count mismatch unexpectedly accepted: {name}"
        );
    }
}

#[test]
fn accepts_pinned_zero_count_section_omission_cases() {
    let function_zero_without_code = module("03 01 00");
    let code_zero_without_function = module("0a 01 00");

    let parsed = parse_module(&function_zero_without_code)
        .expect("zero defined functions do not require an explicit code section");
    assert!(parsed.function_type_indices.is_empty());
    assert!(parsed.code.is_empty());

    let parsed = parse_module(&code_zero_without_function)
        .expect("zero code bodies do not require an explicit function section");
    assert!(parsed.function_type_indices.is_empty());
    assert!(parsed.code.is_empty());
}
