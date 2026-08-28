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

fn assert_unreachable_instruction_rejected(instructions: &[u8], expectation: &str) {
    let module = parse_module(&module_with_instructions(instructions))
        .expect("unreachable-instruction boundary fixture must parse");
    let error = match Instance::new(module).expect_err(expectation) {
        RuntimeError::Validation(error) => error,
        other => panic!("expected validator rejection, got {other:?}"),
    };
    assert!(matches!(
        error,
        ValidationError::UnsupportedOpcode {
            function: 0,
            opcode: 0x00,
            ..
        }
    ));
}

#[test]
fn reachable_unreachable_instruction_remains_fail_closed() {
    assert_unreachable_instruction_rejected(
        &[0x00],
        "MVP unreachable instruction must remain outside the admitted surface",
    );
}

#[test]
fn structured_control_does_not_partially_admit_unreachable_instruction() {
    assert_unreachable_instruction_rejected(
        &[
            0x02, 0x40, // block
            0x00, // unreachable instruction
            0x0b, // end block
        ],
        "structured control must not hide unsupported unreachable instruction",
    );
}

#[test]
fn validator_unreachable_frame_does_not_hide_unsupported_unreachable_opcode() {
    assert_unreachable_instruction_rejected(
        &[
            0x02, 0x40, // block
            0x0c, 0x00, // br 0 makes the rest of this frame validator-unreachable
            0x00, // unreachable instruction itself is still unsupported
            0x0b, // end block
        ],
        "stack polymorphism must not partially admit the unreachable instruction opcode",
    );
}

#[test]
fn else_arm_does_not_partially_admit_unreachable_instruction() {
    assert_unreachable_instruction_rejected(
        &[
            0x41, 0x00, // false condition
            0x04, 0x40, // if
            0x05, // else
            0x00, // unreachable instruction in else arm
            0x0b, // end if
        ],
        "an else transition must not hide an unsupported unreachable instruction",
    );
}

#[test]
fn return_induced_unreachable_frame_does_not_hide_unsupported_unreachable_opcode() {
    assert_unreachable_instruction_rejected(
        &[
            0x0f, // return makes the function frame validator-unreachable
            0x00, // unreachable instruction itself is still unsupported
        ],
        "return-induced stack polymorphism must not admit the unsupported unreachable opcode",
    );
}
