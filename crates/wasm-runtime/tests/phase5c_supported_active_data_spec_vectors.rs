use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value, WASM_PAGE_SIZE};

const I32: u8 = 0x7f;
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

fn push_name(bytes: &mut Vec<u8>, name: &str) {
    push_u32(bytes, name.len() as u32);
    bytes.extend_from_slice(name.as_bytes());
}

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn push_body(payload: &mut Vec<u8>, instructions: &[u8]) {
    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    push_u32(payload, body.len() as u32);
    payload.extend_from_slice(&body);
}

fn push_active_data(payload: &mut Vec<u8>, offset: i32, bytes: &[u8]) {
    payload.extend([0x00, 0x41]);
    push_i32(payload, offset);
    payload.push(0x0b);
    push_u32(payload, bytes.len() as u32);
    payload.extend_from_slice(bytes);
}

fn push_explicit_active_data(
    payload: &mut Vec<u8>,
    memory_index: u32,
    offset: i32,
    bytes: &[u8],
) {
    payload.push(0x02);
    push_u32(payload, memory_index);
    payload.push(0x41);
    push_i32(payload, offset);
    payload.push(0x0b);
    push_u32(payload, bytes.len() as u32);
    payload.extend_from_slice(bytes);
}

fn overlapping_data_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(&mut module, 1, &[0x01, 0x60, 0x01, I32, 0x01, I32]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 5, &[0x01, 0x00, 0x01]);

    let mut exports = vec![0x01];
    push_name(&mut exports, "load8");
    exports.extend([0x00, 0x00]);
    push_section(&mut module, 7, &exports);

    let mut code = vec![0x01];
    push_body(&mut code, &[0x20, 0x00, 0x2d, 0x00, 0x00]);
    push_section(&mut module, 10, &code);

    let mut data = vec![0x06];
    push_active_data(&mut data, 0, b"a");
    push_active_data(&mut data, 1, b"b");
    push_active_data(&mut data, 2, b"cde");
    push_active_data(&mut data, 3, b"f");
    push_active_data(&mut data, 2, b"g");
    push_active_data(&mut data, 1, b"h");
    push_section(&mut module, 11, &data);

    module
}

fn explicit_index_data_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(&mut module, 1, &[0x01, 0x60, 0x01, I32, 0x01, I32]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 5, &[0x01, 0x00, 0x01]);

    let mut exports = vec![0x01];
    push_name(&mut exports, "load8");
    exports.extend([0x00, 0x00]);
    push_section(&mut module, 7, &exports);

    let mut code = vec![0x01];
    push_body(&mut code, &[0x20, 0x00, 0x2d, 0x00, 0x00]);
    push_section(&mut module, 10, &code);

    let mut data = vec![0x01];
    push_explicit_active_data(&mut data, 0, 7, b"wasm");
    push_section(&mut module, 11, &data);

    module
}

fn empty_data_at_boundary_module(memory_pages: u32, offset: i32) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    let mut memory = vec![0x01, 0x00];
    push_u32(&mut memory, memory_pages);
    push_section(&mut module, 5, &memory);

    let mut data = vec![0x01];
    push_active_data(&mut data, offset, b"");
    push_section(&mut module, 11, &data);
    module
}

#[test]
fn upstream_overlapping_active_data_segments_apply_in_declaration_order() {
    // WebAssembly/spec test/core/data.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let module = parse_module(&overlapping_data_module()).expect("active data vector must parse");
    let mut vm = Instance::new(module).expect("active data vector must instantiate");

    for (address, expected) in [
        (0, b'a'),
        (1, b'h'),
        (2, b'g'),
        (3, b'f'),
        (4, b'e'),
        (5, 0),
    ] {
        assert_eq!(
            vm.invoke_export("load8", &[Value::I32(address)]).unwrap(),
            Some(Value::I32(i32::from(expected))),
            "unexpected byte at address {address}"
        );
    }
}

#[test]
fn explicit_memory_index_zero_active_data_executes_like_legacy_active_data() {
    let module = parse_module(&explicit_index_data_module())
        .expect("explicit memory-index active data vector must parse");
    assert_eq!(module.data[0].memory_index, 0);

    let mut vm = Instance::new(module).expect("explicit memory-index data must instantiate");
    for (address, expected) in [(7, b'w'), (8, b'a'), (9, b's'), (10, b'm')] {
        assert_eq!(
            vm.invoke_export("load8", &[Value::I32(address)]).unwrap(),
            Some(Value::I32(i32::from(expected))),
            "unexpected byte at address {address}"
        );
    }
}

#[test]
fn upstream_empty_active_data_segment_may_start_at_memory_end() {
    for (memory_pages, offset) in [(0, 0), (1, WASM_PAGE_SIZE as i32)] {
        let module = parse_module(&empty_data_at_boundary_module(memory_pages, offset))
            .expect("empty active data boundary vector must parse");
        Instance::new(module).expect("empty active data at memory end must instantiate");
    }
}
