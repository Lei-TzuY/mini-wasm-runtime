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

fn module_with_instructions(instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x00]);
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

fn assert_nop_rejected(instructions: &[u8], expectation: &str) {
    let module = parse_module(&module_with_instructions(instructions))
        .expect("nop boundary fixture must remain parseable");
    let error = Instance::new(module).expect_err(expectation);
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::UnsupportedOpcode {
            function: 0,
            opcode: 0x01,
            ..
        })
    ));
}

#[test]
fn reachable_nop_remains_fail_closed() {
    assert_nop_rejected(
        &[0x01],
        "nop must remain outside the admitted instruction surface",
    );
}

#[test]
fn structured_control_does_not_partially_admit_nop() {
    assert_nop_rejected(
        &[
            0x02, 0x40, // block
            0x01, // nop
            0x0b,
        ],
        "structured control must not partially admit unsupported nop",
    );
}

#[test]
fn unreachable_polymorphism_does_not_hide_unsupported_nop() {
    assert_nop_rejected(
        &[
            0x02, 0x40, // block
            0x0c, 0x00, // br 0 makes the rest of this frame unreachable
            0x01, // nop must still fail closed
            0x0b,
        ],
        "unreachable code must still reject unsupported nop",
    );
}
