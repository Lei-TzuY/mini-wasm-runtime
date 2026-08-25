use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};

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

fn push_active_element(payload: &mut Vec<u8>, offset: i32, function_indices: &[u32]) {
    payload.extend([0x00, 0x41]);
    push_i32(payload, offset);
    payload.push(0x0b);
    push_u32(payload, function_indices.len() as u32);
    for &function_index in function_indices {
        push_u32(payload, function_index);
    }
}

fn sparse_element_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x01, I32]);
    push_section(&mut module, 3, &[0x04, 0x00, 0x00, 0x00, 0x00]);
    push_section(&mut module, 4, &[0x01, 0x70, 0x00, 0x0a]);

    let mut exports = vec![0x02];
    for (name, function_index) in [("call-7", 2u32), ("call-9", 3)] {
        push_name(&mut exports, name);
        exports.push(0x00);
        push_u32(&mut exports, function_index);
    }
    push_section(&mut module, 7, &exports);

    let mut elements = vec![0x02];
    push_active_element(&mut elements, 7, &[0]);
    push_active_element(&mut elements, 9, &[1]);
    push_section(&mut module, 9, &elements);

    let mut code = vec![0x04];
    let mut target_a = vec![0x41];
    push_i32(&mut target_a, 65);
    push_body(&mut code, &target_a);
    let mut target_b = vec![0x41];
    push_i32(&mut target_b, 66);
    push_body(&mut code, &target_b);
    push_body(&mut code, &[0x41, 0x07, 0x11, 0x00, 0x00]);
    push_body(&mut code, &[0x41, 0x09, 0x11, 0x00, 0x00]);
    push_section(&mut module, 10, &code);

    module
}

fn empty_element_at_boundary_module(table_size: u32, offset: i32) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    let mut table = vec![0x01, 0x70, 0x00];
    push_u32(&mut table, table_size);
    push_section(&mut module, 4, &table);

    let mut elements = vec![0x01];
    push_active_element(&mut elements, offset, &[]);
    push_section(&mut module, 9, &elements);
    module
}

#[test]
fn upstream_sparse_active_elements_initialize_exact_table_slots() {
    // WebAssembly/spec test/core/elem.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let module = parse_module(&sparse_element_module()).expect("active element vector must parse");
    let mut vm = Instance::new(module).expect("active element vector must instantiate");

    assert_eq!(
        vm.invoke_export("call-7", &[]).unwrap(),
        Some(Value::I32(65))
    );
    assert_eq!(
        vm.invoke_export("call-9", &[]).unwrap(),
        Some(Value::I32(66))
    );
}

#[test]
fn upstream_empty_active_element_segment_may_start_at_table_end() {
    for (table_size, offset) in [(0, 0), (20, 20)] {
        let module = parse_module(&empty_element_at_boundary_module(table_size, offset))
            .expect("empty active element boundary vector must parse");
        Instance::new(module).expect("empty active element at table end must instantiate");
    }
}
