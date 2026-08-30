use wasm_parser::{parse_module, ParseError};
use wasm_runtime::{Instance, RuntimeError};
use wasm_validator::ValidationError;

const UPSTREAM_SPEC_COMMIT: &str = "fc209c5ed8afc4dfeb9252024d217da3376c7a6f";

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

#[test]
fn upstream_explicit_memory_index_data_mode_preserves_index_and_fails_closed_if_missing() {
    // WebAssembly/spec test/core/data.wast @ the pinned revision contains a
    // crafted mode-2 active segment with memory index 1 specifically to catch
    // parsers that reinterpret the explicit index as a legacy flag or length.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut module, 5, &[0x01, 0x00, 0x00]); // one memory, min=0
    push_section(
        &mut module,
        11,
        &[
            0x01, // one data segment
            0x02, // active mode with explicit memory index
            0x01, // memory index 1
            0x41, 0x00, 0x0b, // i32.const 0; end
            0x00, // empty byte vector
        ],
    );

    let parsed = parse_module(&module).expect("mode-2 data segment must preserve its target index");
    assert_eq!(parsed.data.len(), 1);
    assert_eq!(parsed.data[0].memory_index, 1);
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::DataMemoryOutOfBounds {
                segment: 0,
                memory_index: 1,
            }
        ))
    ));
}

#[test]
fn upstream_passive_data_mode_fails_closed_at_discriminant() {
    // `data.wast` uses passive data segments. Phase 5C accepts active modes 0
    // and 2 only, so a well-formed passive empty segment must still be rejected
    // as mode 1 rather than having its byte-vector length reinterpreted.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(
        &mut module,
        11,
        &[
            0x01, // one data segment
            0x01, // passive mode
            0x00, // empty byte vector
        ],
    );

    assert_eq!(
        parse_module(&module),
        Err(ParseError::UnsupportedDataSegmentMode(1))
    );
}

#[test]
fn upstream_explicit_table_index_element_mode_fails_closed_before_payload_reinterpretation() {
    // WebAssembly/spec test/core/elem.wast includes the binary mode-2 form
    // `(elem (table 0) (i32.const 0) func 0)`. Phase 5C intentionally only
    // accepts legacy active mode 0, so the richer encoding must fail closed.
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x00]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 4, &[0x01, 0x70, 0x00, 0x01]);
    push_section(
        &mut module,
        9,
        &[
            0x01, // one element segment
            0x02, // active mode with explicit table index
            0x00, // table index 0
            0x41, 0x00, 0x0b, // i32.const 0; end
            0x00, // elemkind funcref
            0x01, 0x00, // one function index: 0
        ],
    );

    assert_eq!(
        parse_module(&module),
        Err(ParseError::UnsupportedElementSegmentMode(2))
    );
}

#[test]
fn upstream_nonlegacy_element_modes_fail_closed_at_discriminant() {
    // `elem.wast` exercises passive, declarative, and expression-based element
    // encodings. Their standard binary discriminants are 1 and 3..=7; mode 2
    // has its own crafted regression above. Each payload below is a minimal
    // mode-shaped encoding, but rejection must happen immediately at the mode.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let cases: &[(u32, &[u8])] = &[
        (1, &[0x01, 0x01, 0x00, 0x00]),
        (3, &[0x01, 0x03, 0x00, 0x00]),
        (4, &[0x01, 0x04, 0x41, 0x00, 0x0b, 0x00]),
        (5, &[0x01, 0x05, 0x70, 0x00]),
        (6, &[0x01, 0x06, 0x00, 0x41, 0x00, 0x0b, 0x70, 0x00]),
        (7, &[0x01, 0x07, 0x70, 0x00]),
    ];

    for &(mode, payload) in cases {
        let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        push_section(&mut module, 9, payload);
        assert_eq!(
            parse_module(&module),
            Err(ParseError::UnsupportedElementSegmentMode(mode)),
            "element mode {mode} must fail closed"
        );
    }
}
