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

fn build_module(instructions: &[u8]) -> Vec<u8> {
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

fn build_module_with_typed_block(instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(
        &mut module,
        1,
        &[
            0x02, // two function types
            0x60, 0x00, 0x00, // type 0: [] -> []
            0x60, 0x01, 0x7f, 0x00, // type 1: [i32] -> []
        ],
    );
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

fn validation_error(module: &[u8], expectation: &str) -> ValidationError {
    let module = parse_module(module).expect("fixture must remain structurally parseable");
    match Instance::new(module).expect_err(expectation) {
        RuntimeError::Validation(error) => error,
        other => panic!("expected validator rejection, got {other:?}"),
    }
}

#[test]
fn same_control_frame_keeps_stack_polymorphism_after_unconditional_branch() {
    let module = build_module(&[
        0x0c, 0x00, // br 0: the remainder of the function frame is unreachable
        0x6a, // i32.add may consume polymorphic operands in that same frame
    ]);
    let module = parse_module(&module).expect("fixture must parse");
    Instance::new(module).expect("same-frame unreachable tail must remain stack-polymorphic");
}

#[test]
fn nested_block_after_outer_unreachable_starts_reachable() {
    let module = build_module(&[
        0x0c, 0x00, // br 0: mark the function frame unreachable
        0x02, 0x40, // block: a newly pushed control frame starts reachable
        0x6a, // i32.add therefore underflows this empty block frame
        0x0b,
    ]);
    assert!(matches!(
        validation_error(
            &module,
            "a nested block must not inherit outer stack polymorphism"
        ),
        ValidationError::OperandStackUnderflow { function: 0, .. }
    ));
}

#[test]
fn typed_block_after_outer_unreachable_uses_polymorphism_only_for_entry_params() {
    let module = build_module_with_typed_block(&[
        0x0c, 0x00, // br 0: mark the function frame unreachable
        0x02, 0x01, // block (type 1): outer polymorphism satisfies its i32 parameter
        0x6a, // the new reachable frame has one real i32, so i32.add still underflows
        0x0b,
    ]);
    assert!(matches!(
        validation_error(
            &module,
            "a typed block may consume an outer polymorphic parameter but must start reachable"
        ),
        ValidationError::OperandStackUnderflow {
            function: 0,
            offset: 4
        }
    ));
}

#[test]
fn nested_loop_after_outer_unreachable_starts_reachable() {
    let module = build_module(&[
        0x0c, 0x00, // br 0: mark the function frame unreachable
        0x03, 0x40, // loop: a newly pushed control frame starts reachable
        0x6a, // i32.add therefore underflows this empty loop frame
        0x0b,
    ]);
    assert!(matches!(
        validation_error(
            &module,
            "a nested loop must not inherit outer stack polymorphism"
        ),
        ValidationError::OperandStackUnderflow { function: 0, .. }
    ));
}

#[test]
fn typed_loop_after_outer_unreachable_uses_polymorphism_only_for_entry_params() {
    let module = build_module_with_typed_block(&[
        0x0c, 0x00, // br 0: mark the function frame unreachable
        0x03, 0x01, // loop (type 1): outer polymorphism satisfies its i32 parameter
        0x6a, // the new reachable loop frame has one real i32, so i32.add underflows
        0x0b,
    ]);
    assert!(matches!(
        validation_error(
            &module,
            "a typed loop may consume an outer polymorphic parameter but must start reachable"
        ),
        ValidationError::OperandStackUnderflow {
            function: 0,
            offset: 4
        }
    ));
}

#[test]
fn nested_if_after_outer_unreachable_starts_reachable() {
    let module = build_module(&[
        0x0c, 0x00, // br 0: mark the function frame unreachable
        0x04, 0x40, // if: condition is polymorphic, but the new frame starts reachable
        0x6a, // i32.add therefore underflows this empty then arm
        0x0b,
    ]);
    assert!(matches!(
        validation_error(
            &module,
            "a nested if must not inherit outer stack polymorphism"
        ),
        ValidationError::OperandStackUnderflow { function: 0, .. }
    ));
}

#[test]
fn else_arm_resets_then_arm_unreachable_state() {
    let module = build_module(&[
        0x41, 0x01, // i32.const 1
        0x04, 0x40, // if
        0x0c, 0x00, // br 0: mark only the then arm unreachable
        0x6a, // accepted through same-frame stack polymorphism
        0x05, // else: the alternate arm starts reachable again
        0x6a, // i32.add must now underflow
        0x0b,
    ]);
    assert!(matches!(
        validation_error(
            &module,
            "else must not inherit the then arm's unreachable state"
        ),
        ValidationError::OperandStackUnderflow { function: 0, .. }
    ));
}
