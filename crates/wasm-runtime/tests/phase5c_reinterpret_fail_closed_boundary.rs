use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError};
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

fn reinterpret_module(result_type: u8, instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(
        &mut module,
        1,
        &[
            0x01, // one type
            0x60, 0x00, 0x01, result_type, // () -> result_type
        ],
    );
    push_section(&mut module, 3, &[0x01, 0x00]);

    let mut body = vec![0x00]; // zero local declarations
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn validator_error(bytes: &[u8]) -> ValidationError {
    let module = parse_module(bytes).expect("reinterpret boundary fixture must parse");
    match Instance::new(module)
        .expect_err("reinterpret must remain outside the admitted surface")
    {
        RuntimeError::Validation(error) => error,
        other => panic!("expected validator rejection, got {other:?}"),
    }
}

#[test]
fn all_four_reinterpret_directions_remain_fail_closed() {
    let cases: [(u8, u8, &[u8]); 4] = [
        (
            0xbc,
            0x7f,
            &[0x43, 0x00, 0x00, 0x00, 0x00, 0xbc], // f32.const 0; i32.reinterpret_f32
        ),
        (
            0xbd,
            0x7e,
            &[
                0x44, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xbd,
            ], // f64.const 0; i64.reinterpret_f64
        ),
        (
            0xbe,
            0x7d,
            &[0x41, 0x00, 0xbe], // i32.const 0; f32.reinterpret_i32
        ),
        (
            0xbf,
            0x7c,
            &[0x42, 0x00, 0xbf], // i64.const 0; f64.reinterpret_i64
        ),
    ];

    for (opcode, result_type, instructions) in cases {
        assert!(matches!(
            validator_error(&reinterpret_module(result_type, instructions)),
            ValidationError::UnsupportedOpcode {
                function: 0,
                opcode: actual,
                ..
            } if actual == opcode
        ));
    }
}

#[test]
fn reinterpret_inside_structured_control_is_not_partially_admitted() {
    let module = reinterpret_module(
        0x7f,
        &[
            0x02, 0x7f, // block (result i32)
            0x43, 0x00, 0x00, 0x00, 0x00, // f32.const 0
            0xbc, // i32.reinterpret_f32
            0x0b, // end block
        ],
    );

    assert!(matches!(
        validator_error(&module),
        ValidationError::UnsupportedOpcode {
            function: 0,
            opcode: 0xbc,
            ..
        }
    ));
}
