use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::ValidationError;

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    assert!(payload.len() < 128, "fixture helper uses one-byte lengths");
    module.push(id);
    module.push(payload.len() as u8);
    module.extend(payload);
}

fn global_counter_module(mutable: bool) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x00, 0x01, 0x7f]);
    push_section(&mut bytes, 3, &[0x01, 0x00]);
    push_section(
        &mut bytes,
        6,
        &[0x01, 0x7f, u8::from(mutable), 0x41, 0x07, 0x0b],
    );
    push_section(&mut bytes, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);
    let body = [
        0x00, // local declarations
        0x23, 0x00, // global.get 0
        0x41, 0x01, // i32.const 1
        0x6a, // i32.add
        0x24, 0x00, // global.set 0
        0x23, 0x00, // global.get 0
        0x0b, // end
    ];
    let mut code = vec![0x01, body.len() as u8];
    code.extend(body);
    push_section(&mut bytes, 10, &code);
    bytes
}

fn indirect_module() -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(
        &mut bytes,
        1,
        &[
            0x02, // two types
            0x60, 0x01, 0x7f, 0x01, 0x7f, // type 0: (i32) -> i32
            0x60, 0x02, 0x7f, 0x7f, 0x01, 0x7f, // type 1: (i32, i32) -> i32
        ],
    );
    push_section(&mut bytes, 3, &[0x02, 0x00, 0x01]);
    push_section(&mut bytes, 4, &[0x01, 0x70, 0x00, 0x02]);
    push_section(&mut bytes, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x01]);
    push_section(&mut bytes, 9, &[0x01, 0x00, 0x41, 0x00, 0x0b, 0x01, 0x00]);
    let target = [
        0x00, // locals
        0x20, 0x00, // local.get 0
        0x41, 0x01, // i32.const 1
        0x6a, // add
        0x0b,
    ];
    let caller = [
        0x00, // locals
        0x20, 0x00, // argument to target
        0x20, 0x01, // table element index
        0x11, 0x00, 0x00, // call_indirect type 0 table 0
        0x0b,
    ];
    let mut code = vec![0x02, target.len() as u8];
    code.extend(target);
    code.push(caller.len() as u8);
    code.extend(caller);
    push_section(&mut bytes, 10, &code);
    bytes
}

fn start_module() -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(
        &mut bytes,
        1,
        &[
            0x02, 0x60, 0x00, 0x00, // type 0: () -> ()
            0x60, 0x00, 0x01, 0x7f, // type 1: () -> i32
        ],
    );
    push_section(&mut bytes, 3, &[0x02, 0x00, 0x01]);
    push_section(&mut bytes, 6, &[0x01, 0x7f, 0x01, 0x41, 0x00, 0x0b]);
    push_section(&mut bytes, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x01]);
    push_section(&mut bytes, 8, &[0x00]);
    let start = [0x00, 0x41, 0x2a, 0x24, 0x00, 0x0b];
    let run = [0x00, 0x23, 0x00, 0x0b];
    let mut code = vec![0x02, start.len() as u8];
    code.extend(start);
    code.push(run.len() as u8);
    code.extend(run);
    push_section(&mut bytes, 10, &code);
    bytes
}

#[test]
fn mutable_global_persists_across_invocations() {
    let module = parse_module(&global_counter_module(true)).unwrap();
    let mut vm = Instance::new(module).unwrap();
    assert_eq!(vm.global(0), Some(Value::I32(7)));
    assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(8)));
    assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(9)));
    assert_eq!(vm.global(0), Some(Value::I32(9)));
}

#[test]
fn immutable_global_set_is_rejected_before_execution() {
    let module = parse_module(&global_counter_module(false)).unwrap();
    let error = Instance::new(module).expect_err("immutable global.set must fail validation");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::ImmutableGlobalSet { .. })
    ));
}

#[test]
fn call_indirect_resolves_initialized_funcref() {
    let module = parse_module(&indirect_module()).unwrap();
    let mut vm = Instance::new(module).unwrap();
    assert_eq!(
        vm.invoke_export("run", &[Value::I32(41), Value::I32(0)])
            .unwrap(),
        Some(Value::I32(42))
    );
}

#[test]
fn call_indirect_distinguishes_null_and_oob_slots() {
    let module = parse_module(&indirect_module()).unwrap();
    let mut vm = Instance::new(module).unwrap();
    let null = vm
        .invoke_export("run", &[Value::I32(41), Value::I32(1)])
        .expect_err("slot 1 is uninitialized");
    assert!(matches!(null, RuntimeError::UninitializedTableElement(1)));

    let oob = vm
        .invoke_export("run", &[Value::I32(41), Value::I32(2)])
        .expect_err("slot 2 is outside a two-entry table");
    assert!(matches!(oob, RuntimeError::TableElementOutOfBounds(2)));
}

#[test]
fn call_indirect_type_mismatch_traps_at_runtime() {
    let mut module = parse_module(&indirect_module()).unwrap();
    module.code[1].code = vec![
        0x20, 0x00, // first indirect argument
        0x20, 0x00, // second indirect argument
        0x20, 0x01, // table element index
        0x11, 0x01, 0x00, // call_indirect type 1 table 0
        0x0b,
    ];
    let mut vm = Instance::new(module).expect("statically valid indirect call");
    let error = vm
        .invoke_export("run", &[Value::I32(41), Value::I32(0)])
        .expect_err("table target has type 0, call site expects type 1");
    assert!(matches!(
        error,
        RuntimeError::IndirectCallTypeMismatch {
            expected_type: 1,
            function_index: 0
        }
    ));
}

#[test]
fn invalid_start_signature_is_rejected() {
    let mut module = parse_module(&indirect_module()).unwrap();
    module.start = Some(0);
    let error = Instance::new(module).expect_err("start must be [] -> []");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::InvalidStartSignature { function_index: 0 })
    ));
}

#[test]
fn element_referencing_missing_function_is_rejected() {
    let mut module = parse_module(&indirect_module()).unwrap();
    module.elements[0].function_indices[0] = 99;
    let error = Instance::new(module).expect_err("element function index must exist");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::ElementFunctionOutOfBounds {
            segment: 0,
            function_index: 99
        })
    ));
}

#[test]
fn active_element_oob_is_instantiation_error() {
    let mut module = parse_module(&indirect_module()).unwrap();
    module.tables[0].limits.min = 0;
    let error = Instance::new(module).expect_err("active element must fit initial table");
    assert!(matches!(
        error,
        RuntimeError::ElementSegmentOutOfBounds { .. }
    ));
}

#[test]
fn start_function_runs_after_instance_initialization() {
    let module = parse_module(&start_module()).unwrap();
    let mut vm = Instance::new(module).unwrap();
    assert_eq!(vm.global(0), Some(Value::I32(42)));
    assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(42)));
}
