use wasm_parser::{parse_module, ParseError};

const MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6d];
const VERSION_1: [u8; 4] = [0x01, 0x00, 0x00, 0x00];

#[test]
fn truncated_magic_fails_closed_before_version_parsing() {
    for length in 0..MAGIC.len() {
        assert_eq!(
            parse_module(&MAGIC[..length]),
            Err(ParseError::UnexpectedEof)
        );
    }
}

#[test]
fn invalid_magic_is_reported_before_version_parsing() {
    let invalid = [0xff, 0x61, 0x73, 0x6d];
    assert_eq!(
        parse_module(&invalid),
        Err(ParseError::InvalidMagic(invalid))
    );
}

#[test]
fn truncated_version_fails_closed_after_valid_magic() {
    for length in 0..VERSION_1.len() {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&VERSION_1[..length]);
        assert_eq!(parse_module(&bytes), Err(ParseError::UnexpectedEof));
    }
}

#[test]
fn unsupported_version_preserves_exact_version_bytes() {
    for version in [[0x02, 0x00, 0x00, 0x00], [0x01, 0x00, 0x00, 0x01]] {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&version);
        assert_eq!(
            parse_module(&bytes),
            Err(ParseError::UnsupportedVersion(version))
        );
    }
}

#[test]
fn exact_version_one_header_is_a_valid_empty_module() {
    let mut bytes = MAGIC.to_vec();
    bytes.extend_from_slice(&VERSION_1);
    let module = parse_module(&bytes).expect("exact MVP header must parse as an empty module");

    assert!(module.types.is_empty());
    assert!(module.imports.is_empty());
    assert!(module.function_type_indices.is_empty());
    assert!(module.tables.is_empty());
    assert!(module.memories.is_empty());
    assert!(module.globals.is_empty());
    assert!(module.exports.is_empty());
    assert!(module.start.is_none());
    assert!(module.elements.is_empty());
    assert!(module.code.is_empty());
    assert!(module.data.is_empty());
}
