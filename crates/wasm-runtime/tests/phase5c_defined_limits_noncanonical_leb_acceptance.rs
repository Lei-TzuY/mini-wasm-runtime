use wasm_parser::{parse_module, Limits};

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

fn module_with_section(id: u8, payload: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    module.push(id);
    push_u32(&mut module, payload.len() as u32);
    module.extend_from_slice(payload);
    module
}

#[test]
fn defined_table_limits_accept_noncanonical_u32_leb() {
    let module = module_with_section(
        4,
        &[
            0x01, // one table
            0x70, // funcref
            0x01, // min + max
            0x81, 0x00, // min = 1, non-minimal u32 LEB
            0x82, 0x00, // max = 2, non-minimal u32 LEB
        ],
    );
    let module = parse_module(&module).expect("width-valid non-minimal table limits must parse");
    assert_eq!(
        module.tables[0].limits,
        Limits {
            min: 1,
            max: Some(2)
        }
    );
}

#[test]
fn defined_memory_limits_accept_noncanonical_u32_leb() {
    let module = module_with_section(
        5,
        &[
            0x01, // one memory
            0x01, // min + max
            0x81, 0x00, // min = 1, non-minimal u32 LEB
            0x82, 0x00, // max = 2, non-minimal u32 LEB
        ],
    );
    let module = parse_module(&module).expect("width-valid non-minimal memory limits must parse");
    assert_eq!(
        module.memories[0].limits,
        Limits {
            min: 1,
            max: Some(2)
        }
    );
}
