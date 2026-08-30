use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::ValidationError;

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

fn explicit_element_module(table_index: u32) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x01, 0x7f]);
    push_section(&mut module, 3, &[0x02, 0x00, 0x00]);
    push_section(&mut module, 4, &[0x01, 0x70, 0x00, 0x01]);
    push_section(
        &mut module,
        7,
        &[0x01, 0x04, b'c', b'a', b'l', b'l', 0x00, 0x01],
    );

    let mut elements = vec![0x01, 0x02];
    push_u32(&mut elements, table_index);
    elements.extend([0x41, 0x00, 0x0b, 0x00, 0x01, 0x00]);
    push_section(&mut module, 9, &elements);

    let code = [
        0x02, // two bodies
        0x04, 0x00, 0x41, 0x2a, 0x0b, // target => i32.const 42
        0x07, 0x00, 0x41, 0x00, 0x11, 0x00, 0x00, 0x0b, // call_indirect type 0 at slot 0
    ];
    push_section(&mut module, 10, &code);
    module
}

#[test]
fn explicit_table_index_active_element_mode_two_executes_end_to_end() {
    let parsed =
        parse_module(&explicit_element_module(0)).expect("mode-2 element segment must parse");
    assert_eq!(parsed.elements.len(), 1);
    assert_eq!(parsed.elements[0].table_index, 0);
    assert_eq!(parsed.elements[0].offset, 0);
    assert_eq!(parsed.elements[0].function_indices, vec![0]);

    let mut instance = Instance::new(parsed).expect("mode-2 element segment must instantiate");
    assert_eq!(
        instance.invoke_export("call", &[]).unwrap(),
        Some(Value::I32(42))
    );
}

#[test]
fn explicit_table_index_is_preserved_and_validated_before_instantiation() {
    let parsed =
        parse_module(&explicit_element_module(1)).expect("mode-2 element target must parse");
    assert_eq!(parsed.elements[0].table_index, 1);
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::ElementTableOutOfBounds {
                segment: 0,
                table_index: 1,
            }
        ))
    ));
}
