use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};

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

fn push_table_section(module: &mut Vec<u8>, minimum: u32, maximum: Option<u32>) {
    let mut table = vec![0x01, 0x70];
    match maximum {
        Some(maximum) => {
            table.push(0x01);
            push_u32(&mut table, minimum);
            push_u32(&mut table, maximum);
        }
        None => {
            table.push(0x00);
            push_u32(&mut table, minimum);
        }
    }
    push_section(module, 4, &table);
}

fn push_single_active_element(module: &mut Vec<u8>, offset: i32, function_index: u32) {
    let mut elements = vec![0x01, 0x00, 0x41];
    push_i32(&mut elements, offset);
    elements.push(0x0b);
    elements.push(0x01);
    push_u32(&mut elements, function_index);
    push_section(module, 9, &elements);
}

fn last_slot_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x01, I32]);
    push_section(&mut module, 3, &[0x02, 0x00, 0x00]);
    push_table_section(&mut module, 10, None);

    let mut exports = vec![0x01];
    push_name(&mut exports, "call-last");
    exports.extend([0x00, 0x01]);
    push_section(&mut module, 7, &exports);

    push_single_active_element(&mut module, 9, 0);

    let mut code = vec![0x02];
    push_body(&mut code, &[0x41, 0x2a]);
    push_body(&mut code, &[0x41, 0x09, 0x11, 0x00, 0x00]);
    push_section(&mut module, 10, &code);
    module
}

fn oob_element_module(minimum: u32, maximum: Option<u32>, offset: i32) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x00]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_table_section(&mut module, minimum, maximum);
    push_single_active_element(&mut module, offset, 0);

    let mut code = vec![0x01];
    push_body(&mut code, &[]);
    push_section(&mut module, 10, &code);
    module
}

#[test]
fn upstream_non_empty_active_element_may_fill_last_table_slot() {
    // WebAssembly/spec test/core/elem.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let module = parse_module(&last_slot_module()).expect("last-slot element vector must parse");
    let mut vm = Instance::new(module).expect("last-slot element vector must instantiate");
    assert_eq!(
        vm.invoke_export("call-last", &[]).unwrap(),
        Some(Value::I32(42))
    );
}

#[test]
fn upstream_non_empty_active_elements_use_current_table_bounds_and_unsigned_offsets() {
    for (minimum, maximum, offset, expected_offset) in [
        (0, None, 0, 0u64),
        (10, None, 10, 10),
        (10, Some(20), 10, 10),
        (10, None, -1, u64::from(u32::MAX)),
        (10, None, -10, u64::from(u32::MAX - 9)),
    ] {
        let module = parse_module(&oob_element_module(minimum, maximum, offset))
            .expect("out-of-bounds element vector must parse");
        let error = match Instance::new(module) {
            Ok(_) => panic!("non-empty element at offset {offset} must be out of bounds"),
            Err(error) => error,
        };
        assert!(
            matches!(
                error,
                RuntimeError::ElementSegmentOutOfBounds {
                    segment: 0,
                    offset,
                    length: 1,
                } if offset == expected_offset
            ),
            "unexpected error for minimum={minimum}, maximum={maximum:?}, offset={offset}: {error:?}"
        );
    }
}
