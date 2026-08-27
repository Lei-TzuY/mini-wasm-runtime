use wasm_parser::{parse_module, Constant};
use wasm_runtime::Instance;

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

fn header() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

#[test]
fn defined_numeric_globals_accept_noncanonical_signed_leb_literals() {
    let mut bytes = header();
    push_section(
        &mut bytes,
        6,
        &[
            0x02, // two globals
            0x7f, 0x00, // immutable i32
            0x41, 0x81, 0x00, 0x0b, // i32.const 1; end
            0x7e, 0x00, // immutable i64
            0x42, 0x81, 0x00, 0x0b, // i64.const 1; end
        ],
    );

    let module = parse_module(&bytes).expect("noncanonical signed LEB globals must parse");
    assert_eq!(module.globals[0].init, Constant::I32(1));
    assert_eq!(module.globals[1].init, Constant::I64(1));
    Instance::new(module).expect("noncanonical signed LEB globals must instantiate");
}

#[test]
fn active_data_offset_accepts_noncanonical_signed_leb_i32() {
    let mut bytes = header();
    push_section(&mut bytes, 5, &[0x01, 0x00, 0x01]); // memory min=1
    push_section(
        &mut bytes,
        11,
        &[
            0x01, // one segment
            0x00, // active mode 0
            0x41, 0x81, 0x00, 0x0b, // i32.const 1; end
            0x01, 0xaa, // one payload byte
        ],
    );

    let module = parse_module(&bytes).expect("noncanonical data offset must parse");
    assert_eq!(module.data[0].offset, 1);
    Instance::new(module).expect("noncanonical data offset must instantiate");
}

#[test]
fn active_element_offset_accepts_noncanonical_signed_leb_i32() {
    let mut bytes = header();
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x00, 0x00]); // () -> ()
    push_section(&mut bytes, 3, &[0x01, 0x00]); // one function, type 0
    push_section(&mut bytes, 4, &[0x01, 0x70, 0x00, 0x02]); // funcref table min=2
    push_section(
        &mut bytes,
        9,
        &[
            0x01, // one segment
            0x00, // active mode 0
            0x41, 0x81, 0x00, 0x0b, // i32.const 1; end
            0x01, 0x00, // one function index: 0
        ],
    );
    push_section(&mut bytes, 10, &[0x01, 0x02, 0x00, 0x0b]); // one empty body

    let module = parse_module(&bytes).expect("noncanonical element offset must parse");
    assert_eq!(module.elements[0].offset, 1);
    Instance::new(module).expect("noncanonical element offset must instantiate");
}
