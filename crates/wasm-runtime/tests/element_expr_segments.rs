use wasm_parser::{parse_module, ElementMode, NULL_FUNCREF_INDEX};
use wasm_runtime::{Instance, RuntimeError, Value};

fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

fn base_module(element_payload: &[u8], run_ops: &[u8]) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 0, 1, 0x7f]);
    section(&mut module, 3, &[2, 0, 0]);
    section(&mut module, 4, &[1, 0x70, 0, 1]);
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 1]);
    section(&mut module, 9, element_payload);

    let target = [0, 0x41, 42, 0x0b];
    let mut run = vec![0];
    run.extend_from_slice(run_ops);
    run.push(0x0b);
    let mut code = vec![2, target.len() as u8];
    code.extend_from_slice(&target);
    code.push(run.len() as u8);
    code.extend_from_slice(&run);
    section(&mut module, 10, &code);
    module
}

fn call_indirect_slot_zero() -> Vec<u8> {
    vec![0x41, 0, 0x11, 0, 0]
}

#[test]
fn active_expression_ref_func_dispatches_through_table() {
    // flags=4, active table 0, offset i32.const 0, one (ref.func 0) expression.
    let element = [1, 4, 0x41, 0, 0x0b, 1, 0xd2, 0, 0x0b];
    let module = parse_module(&base_module(&element, &call_indirect_slot_zero())).unwrap();
    assert!(matches!(
        module.elements[0].mode,
        ElementMode::Active {
            table_index: 0,
            offset: 0
        }
    ));
    let mut instance = Instance::new(module).unwrap();
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(42))
    );
}

#[test]
fn active_expression_ref_null_leaves_slot_uninitialized() {
    let element = [1, 4, 0x41, 0, 0x0b, 1, 0xd0, 0x70, 0x0b];
    let module = parse_module(&base_module(&element, &call_indirect_slot_zero())).unwrap();
    assert_eq!(
        module.elements[0].function_indices,
        vec![NULL_FUNCREF_INDEX]
    );
    let mut instance = Instance::new(module).unwrap();
    assert!(matches!(
        instance.invoke_export("run", &[]),
        Err(RuntimeError::UninitializedTableElement(0))
    ));
}

#[test]
fn passive_expression_segment_feeds_table_init() {
    // flags=5, funcref, one ref.func expression.
    let element = [1, 5, 0x70, 1, 0xd2, 0, 0x0b];
    let run = [
        0x41, 0, // destination
        0x41, 0, // source
        0x41, 1, // length
        0xfc, 0x0c, 0, 0, // table.init element 0 table 0
        0x41, 0, 0x11, 0, 0, // call_indirect type 0 table 0
    ];
    let module = parse_module(&base_module(&element, &run)).unwrap();
    let mut instance = Instance::new(module).unwrap();
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(42))
    );
}

#[test]
fn explicit_table_and_declarative_expression_modes_parse() {
    let active = [1, 6, 0, 0x41, 0, 0x0b, 0x70, 1, 0xd2, 0, 0x0b];
    let module = parse_module(&base_module(&active, &call_indirect_slot_zero())).unwrap();
    assert!(matches!(
        module.elements[0].mode,
        ElementMode::Active { table_index: 0, .. }
    ));

    let declarative = [1, 7, 0x70, 1, 0xd2, 0, 0x0b];
    let module = parse_module(&base_module(&declarative, &[0x41, 7])).unwrap();
    assert!(matches!(module.elements[0].mode, ElementMode::Declarative));
}

#[test]
fn expression_segment_rejects_non_funcref_constant() {
    let element = [1, 4, 0x41, 0, 0x0b, 1, 0x41, 0, 0x0b];
    assert!(parse_module(&base_module(&element, &[0x41, 0])).is_err());
}
