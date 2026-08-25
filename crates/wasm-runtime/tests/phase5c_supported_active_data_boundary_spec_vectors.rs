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

fn push_memory_section(module: &mut Vec<u8>, minimum: u32, maximum: Option<u32>) {
    let mut memory = vec![0x01];
    match maximum {
        Some(maximum) => {
            memory.push(0x01);
            push_u32(&mut memory, minimum);
            push_u32(&mut memory, maximum);
        }
        None => {
            memory.push(0x00);
            push_u32(&mut memory, minimum);
        }
    }
    push_section(module, 5, &memory);
}

fn push_active_data(module: &mut Vec<u8>, offset: i32, bytes: &[u8]) {
    let mut data = vec![0x01, 0x00, 0x41];
    push_i32(&mut data, offset);
    data.push(0x0b);
    push_u32(&mut data, bytes.len() as u32);
    data.extend_from_slice(bytes);
    push_section(module, 11, &data);
}

fn last_byte_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x01, I32]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_memory_section(&mut module, 1, None);

    let mut exports = vec![0x01];
    push_name(&mut exports, "load-last");
    exports.extend([0x00, 0x00]);
    push_section(&mut module, 7, &exports);

    let mut code = vec![0x01];
    let mut instructions = vec![0x41];
    push_i32(&mut instructions, 0xffff);
    instructions.extend([0x2d, 0x00, 0x00]);
    push_body(&mut code, &instructions);
    push_section(&mut module, 10, &code);

    push_active_data(&mut module, 0xffff, b"b");
    module
}

fn data_segment_module(minimum: u32, maximum: Option<u32>, offset: i32, bytes: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_memory_section(&mut module, minimum, maximum);
    push_active_data(&mut module, offset, bytes);
    module
}

#[test]
fn upstream_non_empty_active_data_may_fill_last_memory_byte() {
    // WebAssembly/spec test/core/data.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let module = parse_module(&last_byte_module()).expect("last-byte data vector must parse");
    let mut vm = Instance::new(module).expect("last-byte data vector must instantiate");
    assert_eq!(
        vm.invoke_export("load-last", &[]).unwrap(),
        Some(Value::I32(i32::from(b'b')))
    );
}

#[test]
fn upstream_non_empty_active_data_uses_current_memory_bounds_and_unsigned_offsets() {
    for (minimum, maximum, offset, expected_offset) in [
        (0, None, 0, 0u64),
        (0, Some(1), 0, 0),
        (1, None, 0x1_0000, 0x1_0000),
        (1, Some(2), 0x1_0000, 0x1_0000),
        (2, None, 0x2_0000, 0x2_0000),
        (2, Some(3), 0x2_0000, 0x2_0000),
        (1, None, -1, u64::from(u32::MAX)),
        (2, None, -100, u64::from(u32::MAX - 99)),
    ] {
        let module = parse_module(&data_segment_module(minimum, maximum, offset, b"a"))
            .expect("out-of-bounds data vector must parse");
        let error = match Instance::new(module) {
            Ok(_) => panic!("non-empty data at offset {offset} must be out of bounds"),
            Err(error) => error,
        };
        assert!(
            matches!(
                error,
                RuntimeError::DataSegmentOutOfBounds {
                    segment: 0,
                    offset,
                    length: 1,
                } if offset == expected_offset
            ),
            "unexpected error for minimum={minimum}, maximum={maximum:?}, offset={offset}: {error:?}"
        );
    }
}

#[test]
fn upstream_empty_active_data_beyond_memory_end_is_still_out_of_bounds() {
    // The pinned data.wast directly covers empty@1 with a zero-page memory;
    // the one-page and negative cases lock the same boundary and unsigned-i32
    // rules already exercised by its neighboring non-empty vectors.
    for (minimum, maximum, offset, expected_offset) in [
        (0, None, 1, 1u64),
        (0, Some(1), 1, 1),
        (1, None, 0x1_0001, 0x1_0001),
        (1, Some(2), 0x1_0001, 0x1_0001),
        (1, None, -1, u64::from(u32::MAX)),
    ] {
        let module = parse_module(&data_segment_module(minimum, maximum, offset, b""))
            .expect("empty out-of-bounds data vector must parse");
        let error = Instance::new(module)
            .expect_err("empty active data beyond current memory end must fail instantiation");
        assert!(
            matches!(
                error,
                RuntimeError::DataSegmentOutOfBounds {
                    segment: 0,
                    offset,
                    length: 0,
                } if offset == expected_offset
            ),
            "unexpected error for minimum={minimum}, maximum={maximum:?}, offset={offset}: {error:?}"
        );
    }
}
