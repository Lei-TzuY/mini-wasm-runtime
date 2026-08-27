use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};
use wasm_validator::{validate, ValidationError};

const I32: u8 = 0x7f;

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

fn module_with_result(result_type: Option<u8>, instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let mut ty = vec![0x01, 0x60, 0x00];
    match result_type {
        Some(result) => ty.extend_from_slice(&[0x01, result]),
        None => ty.push(0x00),
    }
    push_section(&mut module, 1, &ty);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);

    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn execute(instructions: &[u8], expected: Option<Value>) {
    let module = parse_module(&module_with_result(expected.map(|_| I32), instructions))
        .expect("drop vector must parse");
    validate(&module).expect("drop vector must validate");
    let mut instance = Instance::new(module).expect("drop vector must instantiate");
    assert_eq!(instance.invoke_export("run", &[]).unwrap(), expected);
}

#[test]
fn drop_discards_each_numeric_type_and_preserves_lower_result() {
    execute(
        &[
            0x41, 0x2a, // retained i32 result
            0x41, 0x07, // discarded i32
            0x1a,
        ],
        Some(Value::I32(42)),
    );
    execute(
        &[
            0x41, 0x2a, // retained i32 result
            0x42, 0x07, // discarded i64
            0x1a,
        ],
        Some(Value::I32(42)),
    );

    let mut f32_case = vec![0x41, 0x2a, 0x43];
    f32_case.extend_from_slice(&f32::from_bits(0x7fc1_2345).to_bits().to_le_bytes());
    f32_case.push(0x1a);
    execute(&f32_case, Some(Value::I32(42)));

    let mut f64_case = vec![0x41, 0x2a, 0x44];
    f64_case.extend_from_slice(&f64::from_bits(0x7ff8_0000_0000_4321).to_bits().to_le_bytes());
    f64_case.push(0x1a);
    execute(&f64_case, Some(Value::I32(42)));
}

#[test]
fn reachable_empty_stack_is_rejected_as_operand_underflow() {
    let module = parse_module(&module_with_result(None, &[0x1a]))
        .expect("underflow vector must parse");
    assert!(matches!(
        validate(&module),
        Err(ValidationError::OperandStackUnderflow {
            function: 0,
            ..
        })
    ));
}

#[test]
fn unreachable_stack_polymorphism_admits_drop_without_fabricating_a_value() {
    let bytes = module_with_result(
        None,
        &[
            0x02, 0x40, // block
            0x0c, 0x00, // br 0 makes the rest of this frame unreachable
            0x1a, // polymorphic drop
            0x0b, // end block
        ],
    );
    let module = parse_module(&bytes).expect("unreachable vector must parse");
    validate(&module).expect("unreachable drop must validate polymorphically");
    let mut instance = Instance::new(module).expect("control-map scan must admit drop");
    assert_eq!(instance.invoke_export("run", &[]).unwrap(), None);
}
