use wasm_parser::{parse_module, ParseError};

#[derive(Debug)]
struct Case {
    name: &'static str,
    bytes: Vec<u8>,
    expected: ParseError,
}

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

fn section_length_leb_overflow() -> Vec<u8> {
    let mut bytes = header();
    bytes.push(0x01);
    bytes.extend_from_slice(&[0xff, 0xff, 0xff, 0xff, 0x10]);
    bytes
}

fn truncated_section_payload() -> Vec<u8> {
    let mut bytes = header();
    bytes.extend_from_slice(&[0x01, 0x02, 0x00]);
    bytes
}

fn invalid_utf8_export_name() -> Vec<u8> {
    let mut bytes = header();
    push_section(&mut bytes, 7, &[0x01, 0x01, 0xff, 0x00, 0x00]);
    bytes
}

fn function_body_missing_end() -> Vec<u8> {
    let mut bytes = header();
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x00, 0x00]);
    push_section(&mut bytes, 3, &[0x01, 0x00]);
    push_section(&mut bytes, 10, &[0x01, 0x01, 0x00]);
    bytes
}

fn const_expr_missing_end() -> Vec<u8> {
    let mut bytes = header();
    push_section(&mut bytes, 6, &[0x01, 0x7f, 0x00, 0x41, 0x00]);
    bytes
}

fn truncated_const_expr_immediate() -> Vec<u8> {
    let mut bytes = header();
    push_section(&mut bytes, 6, &[0x01, 0x7f, 0x00, 0x41, 0x80]);
    bytes
}

fn invalid_const_expr_opcode() -> Vec<u8> {
    let mut bytes = header();
    push_section(&mut bytes, 6, &[0x01, 0x7f, 0x00, 0x01, 0x0b]);
    bytes
}

fn invalid_function_type_tag() -> Vec<u8> {
    let mut bytes = header();
    push_section(&mut bytes, 1, &[0x01, 0x61, 0x00, 0x00]);
    bytes
}

fn unsupported_value_type() -> Vec<u8> {
    let mut bytes = header();
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x01, 0x7b, 0x00]);
    bytes
}

fn invalid_mutability() -> Vec<u8> {
    let mut bytes = header();
    push_section(&mut bytes, 6, &[0x01, 0x7f, 0x02, 0x41, 0x00, 0x0b]);
    bytes
}

fn invalid_reference_type() -> Vec<u8> {
    let mut bytes = header();
    push_section(&mut bytes, 4, &[0x01, 0x6f, 0x00, 0x01]);
    bytes
}

fn unsupported_section() -> Vec<u8> {
    let mut bytes = header();
    push_section(&mut bytes, 13, &[]);
    bytes
}

fn invalid_export_kind() -> Vec<u8> {
    let mut bytes = header();
    push_section(&mut bytes, 7, &[0x01, 0x01, b'x', 0x04, 0x00]);
    bytes
}

fn assert_case(case: Case) {
    match parse_module(&case.bytes) {
        Ok(_) => panic!(
            "malformed binary case {:?} unexpectedly parsed successfully; expected {:?}",
            case.name, case.expected
        ),
        Err(actual) => assert_eq!(
            actual, case.expected,
            "malformed binary case {:?} returned the wrong parse failure",
            case.name
        ),
    }
}

#[test]
fn malformed_binary_inputs_fail_closed_with_precise_parser_errors() {
    let cases = [
        Case {
            name: "truncated module header",
            bytes: vec![0x00, 0x61, 0x73],
            expected: ParseError::UnexpectedEof,
        },
        Case {
            name: "section length LEB128 overflow",
            bytes: section_length_leb_overflow(),
            expected: ParseError::Leb128Overflow,
        },
        Case {
            name: "declared section payload is truncated",
            bytes: truncated_section_payload(),
            expected: ParseError::UnexpectedEof,
        },
        Case {
            name: "invalid UTF-8 export name",
            bytes: invalid_utf8_export_name(),
            expected: ParseError::InvalidUtf8,
        },
        Case {
            name: "function body missing final end",
            bytes: function_body_missing_end(),
            expected: ParseError::FunctionBodyMissingEnd,
        },
        Case {
            name: "constant expression missing end",
            bytes: const_expr_missing_end(),
            expected: ParseError::ConstExprMissingEnd,
        },
        Case {
            name: "constant expression immediate is truncated",
            bytes: truncated_const_expr_immediate(),
            expected: ParseError::UnexpectedEof,
        },
        Case {
            name: "invalid constant expression opcode",
            bytes: invalid_const_expr_opcode(),
            expected: ParseError::InvalidConstExprOpcode(0x01),
        },
        Case {
            name: "invalid function type tag",
            bytes: invalid_function_type_tag(),
            expected: ParseError::InvalidFunctionType(0x61),
        },
        Case {
            name: "unsupported value type",
            bytes: unsupported_value_type(),
            expected: ParseError::UnsupportedValueType(0x7b),
        },
        Case {
            name: "invalid global mutability",
            bytes: invalid_mutability(),
            expected: ParseError::InvalidMutability(0x02),
        },
        Case {
            name: "invalid table reference type",
            bytes: invalid_reference_type(),
            expected: ParseError::InvalidReferenceType(0x6f),
        },
        Case {
            name: "unsupported section id",
            bytes: unsupported_section(),
            expected: ParseError::UnsupportedSection(13),
        },
        Case {
            name: "invalid export kind",
            bytes: invalid_export_kind(),
            expected: ParseError::InvalidExportKind(0x04),
        },
    ];

    for case in cases {
        assert_case(case);
    }
}
