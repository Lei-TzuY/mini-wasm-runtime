use wasm_parser::{parse_module, ParseError};

const U32_MAX_LEB: [u8; 5] = [0xff, 0xff, 0xff, 0xff, 0x0f];

fn header() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
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

fn type_parameter_count_bomb() -> Vec<u8> {
    let mut module = header();
    let mut payload = vec![0x01, 0x60];
    payload.extend_from_slice(&U32_MAX_LEB);
    push_section(&mut module, 1, &payload);
    module
}

fn import_entry_count_bomb() -> Vec<u8> {
    let mut module = header();
    push_section(&mut module, 2, &U32_MAX_LEB);
    module
}

fn element_function_count_bomb() -> Vec<u8> {
    let mut module = header();
    let mut payload = vec![0x01, 0x01, 0x00];
    payload.extend_from_slice(&U32_MAX_LEB);
    push_section(&mut module, 9, &payload);
    module
}

fn local_group_count_bomb() -> Vec<u8> {
    let mut module = header();
    let mut payload = vec![0x01, 0x05];
    payload.extend_from_slice(&U32_MAX_LEB);
    push_section(&mut module, 10, &payload);
    module
}

fn data_segment_count_bomb() -> Vec<u8> {
    let mut module = header();
    push_section(&mut module, 11, &U32_MAX_LEB);
    module
}

#[test]
fn untrusted_vector_counts_do_not_trigger_upfront_giant_allocations() {
    for (name, bytes) in [
        ("type parameter count", type_parameter_count_bomb()),
        ("import entry count", import_entry_count_bomb()),
        ("element function count", element_function_count_bomb()),
        ("local group count", local_group_count_bomb()),
        ("data segment count", data_segment_count_bomb()),
    ] {
        assert_eq!(
            parse_module(&bytes),
            Err(ParseError::UnexpectedEof),
            "{name} bomb should fail on missing encoded entries rather than preallocating from the declared count"
        );
    }
}
