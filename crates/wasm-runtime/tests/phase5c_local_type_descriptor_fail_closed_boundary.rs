use wasm_parser::{parse_module, ParseError, ValueType};

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

fn module_with_body(body: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(body);
    push_section(&mut module, 10, &code);
    module
}

#[test]
fn missing_local_value_type_fails_closed_before_function_end_validation() {
    let module = module_with_body(&[
        0x01, // one local declaration group
        0x01, // one local in the group
    ]);
    assert_eq!(parse_module(&module), Err(ParseError::UnexpectedEof));
}

#[test]
fn reference_local_value_types_remain_unsupported() {
    for unsupported in [0x70, 0x6f] {
        let module = module_with_body(&[
            0x01, // one local declaration group
            0x01, // one local in the group
            unsupported,
            0x0b, // function end
        ]);
        assert_eq!(
            parse_module(&module),
            Err(ParseError::UnsupportedValueType(unsupported))
        );
    }
}

#[test]
fn all_admitted_numeric_local_value_types_parse() {
    let module = module_with_body(&[
        0x04, // four local declaration groups
        0x01, 0x7f, // i32
        0x01, 0x7e, // i64
        0x01, 0x7d, // f32
        0x01, 0x7c, // f64
        0x0b, // function end
    ]);
    let parsed = parse_module(&module).expect("numeric local descriptors must remain admitted");
    assert_eq!(
        parsed.code[0].locals,
        vec![
            (1, ValueType::I32),
            (1, ValueType::I64),
            (1, ValueType::F32),
            (1, ValueType::F64),
        ]
    );
}
