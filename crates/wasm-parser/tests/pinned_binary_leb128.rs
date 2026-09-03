use wasm_parser::{decode_i32, decode_i64, decode_u32, parse_module, ParseError};

// Curated from WebAssembly/spec@fc209c5ed8afc4dfeb9252024d217da3376c7a6f
// test/core/binary-leb128.wast. This focuses only on parser-owned LEB128
// boundaries already inside the current MVP surface; proposal-dependent cases
// and instruction-immediate validation owned by later layers stay out of scope.

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
fn accepts_pinned_non_minimal_leb128_encodings() {
    let unsigned = [
        (&[0x80, 0x00][..], 0u32, 2usize),
        (&[0x82, 0x00][..], 2, 2),
        (&[0x80, 0x80, 0x00][..], 0, 3),
        (&[0x82, 0x80, 0x80, 0x80, 0x00][..], 2, 5),
    ];
    for (encoded, expected, used) in unsigned {
        assert_eq!(decode_u32(encoded), Ok((expected, used)));
    }

    let signed_i32 = [
        (&[0x80, 0x00][..], 0i32, 2usize),
        (&[0xff, 0x7f][..], -1, 2),
        (&[0x80, 0x80, 0x80, 0x80, 0x00][..], 0, 5),
        (&[0xff, 0xff, 0xff, 0xff, 0x7f][..], -1, 5),
    ];
    for (encoded, expected, used) in signed_i32 {
        assert_eq!(decode_i32(encoded), Ok((expected, used)));
    }

    let signed_i64 = [
        (&[0x80, 0x00][..], 0i64, 2usize),
        (&[0xff, 0x7f][..], -1, 2),
        (
            &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00][..],
            0,
            10,
        ),
        (
            &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f][..],
            -1,
            10,
        ),
    ];
    for (encoded, expected, used) in signed_i64 {
        assert_eq!(decode_i64(encoded), Ok((expected, used)));
    }
}

#[test]
fn rejects_pinned_overlong_and_unused_bit_leb128_encodings() {
    let unsigned_overlong = [
        &[0x82, 0x80, 0x80, 0x80, 0x80, 0x00][..],
        &[0x80, 0x80, 0x80, 0x80, 0x80, 0x00][..],
    ];
    for encoded in unsigned_overlong {
        assert_eq!(decode_u32(encoded), Err(ParseError::InvalidLeb128));
    }

    let unsigned_too_large = [
        &[0x82, 0x80, 0x80, 0x80, 0x10][..],
        &[0x81, 0x80, 0x80, 0x80, 0x40][..],
    ];
    for encoded in unsigned_too_large {
        assert_eq!(decode_u32(encoded), Err(ParseError::Leb128Overflow));
    }

    let signed_i32_overlong = [
        &[0x80, 0x80, 0x80, 0x80, 0x80, 0x00][..],
        &[0xff, 0xff, 0xff, 0xff, 0xff, 0x7f][..],
    ];
    for encoded in signed_i32_overlong {
        assert_eq!(decode_i32(encoded), Err(ParseError::InvalidLeb128));
    }

    let signed_i32_too_large = [
        &[0x80, 0x80, 0x80, 0x80, 0x70][..],
        &[0xff, 0xff, 0xff, 0xff, 0x0f][..],
    ];
    for encoded in signed_i32_too_large {
        assert_eq!(decode_i32(encoded), Err(ParseError::Leb128Overflow));
    }

    let signed_i64_overlong = [
        &[
            0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00,
        ][..],
        &[
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f,
        ][..],
    ];
    for encoded in signed_i64_overlong {
        assert_eq!(decode_i64(encoded), Err(ParseError::InvalidLeb128));
    }
}

#[test]
fn rejects_overlong_u32s_in_parser_owned_binary_fields() {
    // Keep each section payload long enough to contain the malformed field so
    // this checks the LEB128 decoder boundary itself rather than an earlier
    // section-payload EOF. The field encodings are the pinned upstream forms.
    let cases = [
        (
            "memory minimum",
            "05 08 01 00 82 80 80 80 80 00",
        ),
        (
            "type parameter count",
            "01 0c 01 60 82 80 80 80 80 00 7f 7e 01 7f",
        ),
        (
            "import module-name length",
            "01 05 01 60 01 7f 00 02 1b 01 88 80 80 80 80 00 73 70 65 63 74 65 73 74 09 70 72 69 6e 74 5f 69 33 32 00 00",
        ),
        (
            "function type index",
            "01 04 01 60 00 00 03 07 01 80 80 80 80 80 00 0a 04 01 02 00 0b",
        ),
        (
            "export name length",
            "01 04 01 60 00 00 03 02 01 00 07 0b 01 82 80 80 80 80 00 66 31 00 00 0a 04 01 02 00 0b",
        ),
        (
            "code function count",
            "01 04 01 60 00 00 03 02 01 00 0a 09 81 80 80 80 80 00 02 00 0b",
        ),
    ];

    for (name, tail) in cases {
        assert_eq!(
            parse_module(&module(tail)),
            Err(ParseError::InvalidLeb128),
            "pinned binary-leb128 case unexpectedly accepted: {name}"
        );
    }
}
