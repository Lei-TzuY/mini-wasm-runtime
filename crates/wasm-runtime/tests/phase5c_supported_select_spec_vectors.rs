use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};
use wasm_validator::{validate, ValidationError};

const I32: u8 = 0x7f;
const I64: u8 = 0x7e;
const F32: u8 = 0x7d;
const F64: u8 = 0x7c;
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

fn upstream_global_select_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x01, I32]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(
        &mut module,
        6,
        &[
            0x01, // one global
            I32, 0x01, // mutable i32
            0x41, 0x06, 0x0b, // i32.const 6; end
        ],
    );
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);
    let body = [
        0x00, // no locals
        0x23, 0x00, // global.get 0 => first value (6)
        0x41, 0x02, // i32.const 2 => second value
        0x41, 0x03, // i32.const 3 => non-zero condition
        0x1b, // select => first value
        0x0b,
    ];
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn execute(bytes: &[u8], expected: Value) {
    let module = parse_module(bytes).expect("select vector must parse");
    validate(&module).expect("select vector must validate");
    let mut instance = Instance::new(module).expect("select vector must instantiate");
    assert_eq!(instance.invoke_export("run", &[]).unwrap(), Some(expected));
}

fn f32_select(condition: i32) -> Vec<u8> {
    let mut instructions = vec![0x43];
    instructions.extend_from_slice(&11.5f32.to_bits().to_le_bytes());
    instructions.push(0x43);
    instructions.extend_from_slice(&22.5f32.to_bits().to_le_bytes());
    instructions.extend_from_slice(&[0x41, condition as u8, 0x1b]);
    module_with_result(Some(F32), &instructions)
}

fn f64_select(condition: i32) -> Vec<u8> {
    let mut instructions = vec![0x44];
    instructions.extend_from_slice(&11.5f64.to_bits().to_le_bytes());
    instructions.push(0x44);
    instructions.extend_from_slice(&22.5f64.to_bits().to_le_bytes());
    instructions.extend_from_slice(&[0x41, condition as u8, 0x1b]);
    module_with_result(Some(F64), &instructions)
}

#[test]
fn pinned_upstream_global_get_select_context_executes() {
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);
    execute(&upstream_global_select_module(), Value::I32(6));
}

#[test]
fn select_executes_both_directions_for_all_numeric_types() {
    execute(
        &module_with_result(Some(I32), &[0x41, 0x0b, 0x41, 0x16, 0x41, 0x01, 0x1b]),
        Value::I32(11),
    );
    execute(
        &module_with_result(Some(I32), &[0x41, 0x0b, 0x41, 0x16, 0x41, 0x00, 0x1b]),
        Value::I32(22),
    );
    execute(
        &module_with_result(Some(I64), &[0x42, 0x0b, 0x42, 0x16, 0x41, 0x01, 0x1b]),
        Value::I64(11),
    );
    execute(
        &module_with_result(Some(I64), &[0x42, 0x0b, 0x42, 0x16, 0x41, 0x00, 0x1b]),
        Value::I64(22),
    );
    execute(&f32_select(1), Value::F32(11.5));
    execute(&f32_select(0), Value::F32(22.5));
    execute(&f64_select(1), Value::F64(11.5));
    execute(&f64_select(0), Value::F64(22.5));
}

#[test]
fn select_rejects_mismatched_value_types() {
    let module = parse_module(&module_with_result(
        Some(I32),
        &[
            0x41, 0x01, // i32 first value
            0x42, 0x02, // i64 second value
            0x41, 0x00, // i32 condition
            0x1b,
        ],
    ))
    .expect("mismatch vector must parse");
    assert!(matches!(
        validate(&module),
        Err(ValidationError::TypeMismatch {
            expected: wasm_parser::ValueType::I64,
            actual: wasm_parser::ValueType::I32,
            ..
        })
    ));
}

#[test]
fn select_rejects_non_i32_condition() {
    let module = parse_module(&module_with_result(
        Some(I32),
        &[
            0x41, 0x01, // first value
            0x41, 0x02, // second value
            0x42, 0x00, // i64 condition
            0x1b,
        ],
    ))
    .expect("condition mismatch vector must parse");
    assert!(matches!(
        validate(&module),
        Err(ValidationError::TypeMismatch {
            expected: wasm_parser::ValueType::I32,
            actual: wasm_parser::ValueType::I64,
            ..
        })
    ));
}

#[test]
fn select_rejects_reachable_operand_underflow() {
    let module = parse_module(&module_with_result(
        None,
        &[
            0x41, 0x01, // only one candidate value
            0x41, 0x00, // condition
            0x1b,
        ],
    ))
    .expect("underflow vector must parse");
    assert!(matches!(
        validate(&module),
        Err(ValidationError::OperandStackUnderflow { .. })
    ));
}

#[test]
fn typed_select_remains_explicitly_fail_closed() {
    let module = parse_module(&module_with_result(
        Some(I32),
        &[
            0x41, 0x01, // first value
            0x41, 0x02, // second value
            0x41, 0x00, // condition
            0x1c, 0x01, I32, // typed select [i32]
        ],
    ))
    .expect("typed-select boundary vector must parse");
    assert!(matches!(
        validate(&module),
        Err(ValidationError::UnsupportedOpcode { opcode: 0x1c, .. })
    ));
}

#[test]
fn select_preserves_polymorphic_unreachable_stack_semantics() {
    for instructions in [
        vec![
            0x02, 0x40, // block
            0x0c, 0x00, // br 0 => remainder unreachable
            0x1b, // no concrete operands; polymorphic stack supplies all three
            0x0b, // end block
        ],
        vec![
            0x02, 0x40, // block
            0x0c, 0x00, // br 0
            0x42, 0x07, // concrete i64 second value in unreachable code
            0x41, 0x01, // concrete i32 condition
            0x1b, // first value is polymorphic and unifies with i64
            0x0b,
        ],
    ] {
        let module = parse_module(&module_with_result(None, &instructions))
            .expect("unreachable select vector must parse");
        validate(&module).expect("unreachable select must validate polymorphically");
        let mut instance = Instance::new(module).expect("validated vector must instantiate");
        assert_eq!(instance.invoke_export("run", &[]).unwrap(), None);
    }
}
