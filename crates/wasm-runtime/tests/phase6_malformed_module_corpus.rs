use wasm_parser::{parse_module, Export, ExportKind, Module};
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::{validate, ValidationError};

const I32: u8 = 0x7f;
const F32: u8 = 0x7d;

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

fn function_module(
    params: &[u8],
    results: &[u8],
    instructions: &[u8],
    memory_minimum: Option<u32>,
) -> Vec<u8> {
    let mut bytes = header();

    let mut ty = vec![0x01, 0x60];
    push_u32(&mut ty, params.len() as u32);
    ty.extend_from_slice(params);
    push_u32(&mut ty, results.len() as u32);
    ty.extend_from_slice(results);
    push_section(&mut bytes, 1, &ty);
    push_section(&mut bytes, 3, &[0x01, 0x00]);

    if let Some(minimum) = memory_minimum {
        let mut memory = vec![0x01, 0x00];
        push_u32(&mut memory, minimum);
        push_section(&mut bytes, 5, &memory);
    }

    push_section(&mut bytes, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);

    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend(body);
    push_section(&mut bytes, 10, &code);
    bytes
}

fn immutable_global_module() -> Vec<u8> {
    let mut bytes = header();
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x00, 0x00]);
    push_section(&mut bytes, 3, &[0x01, 0x00]);
    push_section(&mut bytes, 6, &[0x01, I32, 0x00, 0x41, 0x00, 0x0b]);
    push_section(&mut bytes, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);
    let body = [0x00, 0x41, 0x01, 0x24, 0x00, 0x0b];
    let mut code = vec![0x01, body.len() as u8];
    code.extend(body);
    push_section(&mut bytes, 10, &code);
    bytes
}

fn indirect_module() -> Vec<u8> {
    let mut bytes = header();
    push_section(
        &mut bytes,
        1,
        &[
            0x02,
            0x60, 0x01, I32, 0x01, I32,
            0x60, 0x02, I32, I32, 0x01, I32,
        ],
    );
    push_section(&mut bytes, 3, &[0x02, 0x00, 0x01]);
    push_section(&mut bytes, 4, &[0x01, 0x70, 0x00, 0x02]);
    push_section(&mut bytes, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x01]);
    push_section(&mut bytes, 9, &[0x01, 0x00, 0x41, 0x00, 0x0b, 0x01, 0x00]);

    let target = [0x00, 0x20, 0x00, 0x41, 0x01, 0x6a, 0x0b];
    let caller = [0x00, 0x20, 0x00, 0x20, 0x01, 0x11, 0x00, 0x00, 0x0b];
    let mut code = vec![0x02, target.len() as u8];
    code.extend(target);
    code.push(caller.len() as u8);
    code.extend(caller);
    push_section(&mut bytes, 10, &code);
    bytes
}

fn parsed(bytes: Vec<u8>) -> Module {
    parse_module(&bytes).expect("corpus seed must parse before validation mutation")
}

fn expect_validation(module: &Module, predicate: impl FnOnce(&ValidationError) -> bool, name: &str) {
    let error = validate(module).unwrap_err_or_else(|| panic!("{name}: malformed module validated"));
    assert!(predicate(&error), "{name}: unexpected validation error: {error:?}");
}

trait ResultExt<T, E> {
    fn unwrap_err_or_else(self, f: impl FnOnce() -> E) -> E;
}

impl<T, E> ResultExt<T, E> for Result<T, E> {
    fn unwrap_err_or_else(self, f: impl FnOnce() -> E) -> E {
        match self {
            Ok(_) => f(),
            Err(error) => error,
        }
    }
}

#[test]
fn malformed_modules_fail_in_validation_with_specific_classes() {
    let seed = || parsed(function_module(&[], &[], &[], None));

    let mut function_code_mismatch = seed();
    function_code_mismatch.code.clear();
    expect_validation(
        &function_code_mismatch,
        |error| matches!(error, ValidationError::FunctionCodeLengthMismatch { functions: 1, bodies: 0 }),
        "function/code length mismatch",
    );

    let mut type_index_oob = seed();
    type_index_oob.function_type_indices[0] = 99;
    expect_validation(
        &type_index_oob,
        |error| matches!(error, ValidationError::TypeIndexOutOfBounds { function: 0, type_index: 99 }),
        "function type index out of bounds",
    );

    let mut duplicate_export = seed();
    duplicate_export.exports.push(Export {
        name: "run".to_owned(),
        kind: ExportKind::Function,
        index: 0,
    });
    expect_validation(
        &duplicate_export,
        |error| matches!(error, ValidationError::DuplicateExportName(name) if name == "run"),
        "duplicate export",
    );

    let mut function_export_oob = seed();
    function_export_oob.exports[0].index = 99;
    expect_validation(
        &function_export_oob,
        |error| matches!(error, ValidationError::FunctionExportOutOfBounds { function_index: 99, .. }),
        "function export out of bounds",
    );

    let local_oob = parsed(function_module(&[], &[I32], &[0x20, 0x00], None));
    expect_validation(
        &local_oob,
        |error| matches!(error, ValidationError::LocalIndexOutOfBounds { local_index: 0, .. }),
        "local index out of bounds",
    );

    let call_oob = parsed(function_module(&[], &[], &[0x10, 0x01], None));
    expect_validation(
        &call_oob,
        |error| matches!(error, ValidationError::CallTargetOutOfBounds { target: 1, .. }),
        "call target out of bounds",
    );

    let memory_without_memory = parsed(function_module(
        &[],
        &[I32],
        &[0x41, 0x00, 0x28, 0x02, 0x00],
        None,
    ));
    expect_validation(
        &memory_without_memory,
        |error| matches!(error, ValidationError::MemoryInstructionWithoutMemory { .. }),
        "memory instruction without memory",
    );

    let invalid_alignment = parsed(function_module(
        &[],
        &[I32],
        &[0x41, 0x00, 0x28, 0x03, 0x00],
        Some(1),
    ));
    expect_validation(
        &invalid_alignment,
        |error| matches!(error, ValidationError::InvalidMemoryAlignment { alignment: 3, maximum: 2, .. }),
        "invalid memory alignment",
    );

    let branch_oob = parsed(function_module(&[], &[], &[0x0c, 0x01], None));
    expect_validation(
        &branch_oob,
        |error| matches!(error, ValidationError::BranchDepthOutOfBounds { depth: 1, .. }),
        "branch depth out of bounds",
    );

    let unexpected_else = parsed(function_module(&[], &[], &[0x05], None));
    expect_validation(
        &unexpected_else,
        |error| matches!(error, ValidationError::UnexpectedElse { .. }),
        "unexpected else",
    );

    let missing_else = parsed(function_module(
        &[],
        &[I32],
        &[0x41, 0x01, 0x04, I32, 0x41, 0x07, 0x0b],
        None,
    ));
    expect_validation(
        &missing_else,
        |error| matches!(error, ValidationError::MissingElseForResult { .. }),
        "missing else for result",
    );

    let immutable_global_set = parsed(immutable_global_module());
    expect_validation(
        &immutable_global_set,
        |error| matches!(error, ValidationError::ImmutableGlobalSet { global_index: 0, .. }),
        "immutable global set",
    );

    let mut invalid_start = parsed(function_module(&[], &[I32], &[0x41, 0x00], None));
    invalid_start.start = Some(0);
    expect_validation(
        &invalid_start,
        |error| matches!(error, ValidationError::InvalidStartSignature { function_index: 0 }),
        "invalid start signature",
    );

    let stack_underflow = parsed(function_module(&[], &[I32], &[0x6a], None));
    expect_validation(
        &stack_underflow,
        |error| matches!(error, ValidationError::OperandStackUnderflow { .. }),
        "operand stack underflow",
    );

    let type_mismatch = parsed(function_module(
        &[],
        &[I32],
        &[0x43, 0x00, 0x00, 0x00, 0x00, 0x45],
        None,
    ));
    expect_validation(
        &type_mismatch,
        |error| matches!(error, ValidationError::TypeMismatch { expected, actual, .. } if *expected == wasm_parser::ValueType::I32 && *actual == wasm_parser::ValueType::F32),
        "typed operand mismatch",
    );

    let mut invalid_memory_limits = parsed(function_module(&[], &[], &[], Some(1)));
    invalid_memory_limits.memories[0].limits.max = Some(0);
    expect_validation(
        &invalid_memory_limits,
        |error| matches!(error, ValidationError::InvalidMemoryLimits { memory: 0, min: 1, max: 0 }),
        "invalid memory limits",
    );
}

#[test]
fn runtime_traps_only_after_parse_validation_and_instantiation_succeed() {
    let module = parsed(indirect_module());
    validate(&module).expect("indirect-call corpus seed must validate");
    let mut instance = Instance::new(module).expect("indirect-call corpus seed must instantiate");

    let null = instance
        .invoke_export("run", &[Value::I32(41), Value::I32(1)])
        .expect_err("uninitialized table slot must trap dynamically");
    assert!(matches!(null, RuntimeError::UninitializedTableElement(1)));

    let oob = instance
        .invoke_export("run", &[Value::I32(41), Value::I32(2)])
        .expect_err("out-of-range table slot must trap dynamically");
    assert!(matches!(oob, RuntimeError::TableElementOutOfBounds(2)));

    let memory_store = parsed(function_module(
        &[I32, I32],
        &[],
        &[0x20, 0x00, 0x20, 0x01, 0x36, 0x02, 0x00],
        Some(1),
    ));
    validate(&memory_store).expect("memory OOB fixture must be statically valid");
    let mut memory_instance = Instance::new(memory_store).expect("memory OOB fixture must instantiate");
    let error = memory_instance
        .invoke_export("run", &[Value::I32(65_535), Value::I32(7)])
        .expect_err("four-byte store at byte 65535 must trap");
    assert!(matches!(error, RuntimeError::MemoryOutOfBounds { width: 4, .. }));

    let conversion = parsed(function_module(&[F32], &[I32], &[0x20, 0x00, 0xa8], None));
    validate(&conversion).expect("conversion fixture must be statically valid");
    let mut conversion_instance = Instance::new(conversion).expect("conversion fixture must instantiate");
    let error = conversion_instance
        .invoke_export("run", &[Value::F32(f32::NAN)])
        .expect_err("NaN truncation must trap dynamically");
    assert!(matches!(error, RuntimeError::InvalidConversionToInteger));
}
