use wasm_parser::{parse_module, ParseError};
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::{validate, ValidationError};

const I32: u8 = 0x7f;
const F32: u8 = 0x7d;

#[derive(Debug)]
enum ExpectedFailure {
    InstantiateUnresolvedFunctionImport,
    InstantiateUnresolvedMemoryImport,
    InstantiateDataSegmentOutOfBounds,
    ExecuteIntegerDivisionByZero,
    ExecuteIntegerOverflow,
    ExecuteMemoryOutOfBounds,
    ExecuteInvalidConversionToInteger,
}

#[derive(Debug)]
struct Case {
    name: &'static str,
    bytes: Vec<u8>,
    args: Vec<Value>,
    expected: ExpectedFailure,
}

#[derive(Debug)]
enum ObservedFailure {
    Parse(ParseError),
    Validate(ValidationError),
    Instantiate(RuntimeError),
    Execute(RuntimeError),
    Success,
}

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

fn push_name(bytes: &mut Vec<u8>, name: &str) {
    push_u32(bytes, name.len() as u32);
    bytes.extend_from_slice(name.as_bytes());
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

    push_section(
        &mut bytes,
        7,
        &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00],
    );

    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend(body);
    push_section(&mut bytes, 10, &code);
    bytes
}

fn unresolved_function_import_module() -> Vec<u8> {
    let mut bytes = header();
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x00, 0x00]);

    let mut import = vec![0x01];
    push_name(&mut import, "env");
    push_name(&mut import, "missing");
    import.push(0x00);
    import.push(0x00);
    push_section(&mut bytes, 2, &import);

    let mut export = vec![0x01];
    push_name(&mut export, "run");
    export.push(0x00);
    export.push(0x00);
    push_section(&mut bytes, 7, &export);
    bytes
}

fn unresolved_memory_import_module() -> Vec<u8> {
    let mut bytes = header();
    let mut import = vec![0x01];
    push_name(&mut import, "env");
    push_name(&mut import, "memory");
    import.push(0x02);
    import.push(0x00);
    import.push(0x01);
    push_section(&mut bytes, 2, &import);
    bytes
}

fn data_segment_oob_module() -> Vec<u8> {
    let mut bytes = header();
    push_section(&mut bytes, 5, &[0x01, 0x00, 0x00]);
    push_section(
        &mut bytes,
        11,
        &[0x01, 0x00, 0x41, 0x00, 0x0b, 0x01, 0xaa],
    );
    bytes
}

fn expects_execution(expected: &ExpectedFailure) -> bool {
    matches!(
        expected,
        ExpectedFailure::ExecuteIntegerDivisionByZero
            | ExpectedFailure::ExecuteIntegerOverflow
            | ExpectedFailure::ExecuteMemoryOutOfBounds
            | ExpectedFailure::ExecuteInvalidConversionToInteger
    )
}

fn observe(case: &Case) -> ObservedFailure {
    let module = match parse_module(&case.bytes) {
        Ok(module) => module,
        Err(error) => return ObservedFailure::Parse(error),
    };

    if let Err(error) = validate(&module) {
        return ObservedFailure::Validate(error);
    }

    let mut instance = match Instance::new(module) {
        Ok(instance) => instance,
        Err(error) => return ObservedFailure::Instantiate(error),
    };

    if expects_execution(&case.expected) {
        return match instance.invoke_export("run", &case.args) {
            Ok(_) => ObservedFailure::Success,
            Err(error) => ObservedFailure::Execute(error),
        };
    }

    ObservedFailure::Success
}

fn assert_case(case: Case) {
    let observed = observe(&case);
    let matched = match (&case.expected, &observed) {
        (
            ExpectedFailure::InstantiateUnresolvedFunctionImport,
            ObservedFailure::Instantiate(RuntimeError::UnresolvedImport { module, name }),
        ) => module == "env" && name == "missing",
        (
            ExpectedFailure::InstantiateUnresolvedMemoryImport,
            ObservedFailure::Instantiate(RuntimeError::UnresolvedMemoryImport { module, name }),
        ) => module == "env" && name == "memory",
        (
            ExpectedFailure::InstantiateDataSegmentOutOfBounds,
            ObservedFailure::Instantiate(RuntimeError::DataSegmentOutOfBounds {
                segment: 0,
                offset: 0,
                length: 1,
            }),
        ) => true,
        (
            ExpectedFailure::ExecuteIntegerDivisionByZero,
            ObservedFailure::Execute(RuntimeError::IntegerDivisionByZero),
        ) => true,
        (
            ExpectedFailure::ExecuteIntegerOverflow,
            ObservedFailure::Execute(RuntimeError::IntegerOverflow),
        ) => true,
        (
            ExpectedFailure::ExecuteMemoryOutOfBounds,
            ObservedFailure::Execute(RuntimeError::MemoryOutOfBounds { width: 4, .. }),
        ) => true,
        (
            ExpectedFailure::ExecuteInvalidConversionToInteger,
            ObservedFailure::Execute(RuntimeError::InvalidConversionToInteger),
        ) => true,
        _ => false,
    };

    assert!(
        matched,
        "runtime negative-conformance case {:?} expected {:?}, observed {:?}",
        case.name, case.expected, observed
    );
}

#[test]
fn instantiation_failures_do_not_leak_into_parse_or_validation() {
    for case in [
        Case {
            name: "unresolved function import",
            bytes: unresolved_function_import_module(),
            args: vec![],
            expected: ExpectedFailure::InstantiateUnresolvedFunctionImport,
        },
        Case {
            name: "unresolved memory import",
            bytes: unresolved_memory_import_module(),
            args: vec![],
            expected: ExpectedFailure::InstantiateUnresolvedMemoryImport,
        },
        Case {
            name: "active data segment out of bounds",
            bytes: data_segment_oob_module(),
            args: vec![],
            expected: ExpectedFailure::InstantiateDataSegmentOutOfBounds,
        },
    ] {
        assert_case(case);
    }
}

#[test]
fn dynamic_traps_require_successful_parse_validation_and_instantiation() {
    for case in [
        Case {
            name: "i32 division by zero",
            bytes: function_module(&[], &[I32], &[0x41, 0x01, 0x41, 0x00, 0x6d], None),
            args: vec![],
            expected: ExpectedFailure::ExecuteIntegerDivisionByZero,
        },
        Case {
            name: "i32 signed division overflow",
            bytes: function_module(
                &[I32, I32],
                &[I32],
                &[0x20, 0x00, 0x20, 0x01, 0x6d],
                None,
            ),
            args: vec![Value::I32(i32::MIN), Value::I32(-1)],
            expected: ExpectedFailure::ExecuteIntegerOverflow,
        },
        Case {
            name: "i32.load dynamic out of bounds",
            bytes: function_module(
                &[],
                &[I32],
                &[0x41, 0x7f, 0x28, 0x02, 0x00],
                Some(1),
            ),
            args: vec![],
            expected: ExpectedFailure::ExecuteMemoryOutOfBounds,
        },
        Case {
            name: "NaN trapping conversion",
            bytes: function_module(&[F32], &[I32], &[0x20, 0x00, 0xa8], None),
            args: vec![Value::F32(f32::NAN)],
            expected: ExpectedFailure::ExecuteInvalidConversionToInteger,
        },
    ] {
        assert_case(case);
    }
}
