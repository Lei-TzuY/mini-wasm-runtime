use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError};
use wasm_validator::ValidationError;

const F32: u8 = 0x7d;
const F64: u8 = 0x7c;

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

fn conversion_module(result_type: u8, instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x01, result_type]);
    push_section(&mut module, 3, &[0x01, 0x00]);

    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn validator_error(bytes: &[u8]) -> ValidationError {
    let module = parse_module(bytes).expect("integer-to-float boundary fixture must parse");
    match Instance::new(module).expect_err("integer-to-float conversion must fail closed") {
        RuntimeError::Validation(error) => error,
        other => panic!("expected validator rejection, got {other:?}"),
    }
}

#[test]
fn all_eight_integer_to_float_conversions_remain_fail_closed() {
    let cases: [(u8, u8, &[u8]); 8] = [
        (0xb2, F32, &[0x41, 0x00]), // f32.convert_i32_s
        (0xb3, F32, &[0x41, 0x00]), // f32.convert_i32_u
        (0xb4, F32, &[0x42, 0x00]), // f32.convert_i64_s
        (0xb5, F32, &[0x42, 0x00]), // f32.convert_i64_u
        (0xb7, F64, &[0x41, 0x00]), // f64.convert_i32_s
        (0xb8, F64, &[0x41, 0x00]), // f64.convert_i32_u
        (0xb9, F64, &[0x42, 0x00]), // f64.convert_i64_s
        (0xba, F64, &[0x42, 0x00]), // f64.convert_i64_u
    ];

    for (opcode, result_type, source) in cases {
        let mut instructions = source.to_vec();
        instructions.push(opcode);
        assert!(matches!(
            validator_error(&conversion_module(result_type, &instructions)),
            ValidationError::UnsupportedOpcode {
                function: 0,
                opcode: actual,
                ..
            } if actual == opcode
        ));
    }
}

#[test]
fn unreachable_polymorphism_does_not_admit_integer_to_float_conversion() {
    let module = conversion_module(
        F32,
        &[
            0x02, F32, // block (result f32)
            0x43, 0x00, 0x00, 0x00, 0x00, // f32.const 0 branch result
            0x0c, 0x00, // br 0
            0x41, 0x00, // unreachable i32.const 0
            0xb2, // f32.convert_i32_s must still fail closed
            0x0b, // end block
        ],
    );

    assert!(matches!(
        validator_error(&module),
        ValidationError::UnsupportedOpcode {
            function: 0,
            opcode: 0xb2,
            ..
        }
    ));
}
