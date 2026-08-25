use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError};

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

fn push_i32(bytes: &mut Vec<u8>, mut value: i32) {
    loop {
        let byte = (value as u8) & 0x7f;
        let sign_bit_set = byte & 0x40 != 0;
        value >>= 7;
        let done = (value == 0 && !sign_bit_set) || (value == -1 && sign_bit_set);
        bytes.push(if done { byte } else { byte | 0x80 });
        if done {
            break;
        }
    }
}

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn empty_active_element_module(table_size: u32, offset: i32) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    let mut table = vec![0x01, 0x70, 0x00];
    push_u32(&mut table, table_size);
    push_section(&mut module, 4, &table);

    let mut elements = vec![0x01, 0x00, 0x41];
    push_i32(&mut elements, offset);
    elements.extend([0x0b, 0x00]);
    push_section(&mut module, 9, &elements);
    module
}

#[test]
fn upstream_empty_active_elements_beyond_table_end_fail_closed() {
    // WebAssembly/spec test/core/elem.wast @ the pinned revision. Exact-end
    // empty segments are legal; any offset beyond the current table length is
    // still out of bounds even though the segment itself has zero elements.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    for (table_size, offset, expected_offset) in [
        (0, 1, 1u64),
        (20, 21, 21),
        (20, -1, u64::from(u32::MAX)),
    ] {
        let module = parse_module(&empty_active_element_module(table_size, offset))
            .expect("empty active element OOB vector must parse");
        let error = Instance::new(module)
            .expect_err("empty active element beyond table end must fail instantiation");
        assert!(
            matches!(
                error,
                RuntimeError::ElementSegmentOutOfBounds {
                    segment: 0,
                    offset,
                    length: 0,
                } if offset == expected_offset
            ),
            "unexpected error for table_size={table_size}, offset={offset}: {error:?}"
        );
    }
}
