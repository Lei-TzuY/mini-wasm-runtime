use wasm_parser::parse_module;

const NONCANONICAL_ZERO: [u8; 2] = [0x80, 0x00];
const NONCANONICAL_ONE: [u8; 2] = [0x81, 0x00];

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
fn active_element_segment_accepts_noncanonical_u32_fields() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&NONCANONICAL_ONE); // one segment
    payload.extend_from_slice(&NONCANONICAL_ZERO); // active mode 0
    payload.extend_from_slice(&[0x41, 0x00, 0x0b]); // i32.const 0; end
    payload.extend_from_slice(&NONCANONICAL_ONE); // one function index
    payload.extend_from_slice(&NONCANONICAL_ZERO); // function index 0

    let module = parse_module(&module_with_section(9, &payload))
        .expect("width-valid non-minimal element LEBs must remain accepted");
    assert_eq!(module.elements.len(), 1);
    assert_eq!(module.elements[0].table_index, 0);
    assert_eq!(module.elements[0].offset, 0);
    assert_eq!(module.elements[0].function_indices, vec![0]);
}

#[test]
fn active_data_segment_accepts_noncanonical_u32_fields() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&NONCANONICAL_ONE); // one segment
    payload.extend_from_slice(&NONCANONICAL_ZERO); // active mode 0
    payload.extend_from_slice(&[0x41, 0x00, 0x0b]); // i32.const 0; end
    payload.extend_from_slice(&NONCANONICAL_ONE); // one payload byte
    payload.push(0xaa);

    let module = parse_module(&module_with_section(11, &payload))
        .expect("width-valid non-minimal data LEBs must remain accepted");
    assert_eq!(module.data.len(), 1);
    assert_eq!(module.data[0].memory_index, 0);
    assert_eq!(module.data[0].offset, 0);
    assert_eq!(module.data[0].bytes, vec![0xaa]);
}
