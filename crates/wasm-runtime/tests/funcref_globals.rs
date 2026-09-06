use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::ValidationError;

fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

fn module(global_init: &[u8], body_ops: &[u8], with_table: bool) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 0, 1, 0x7f]);
    section(&mut module, 3, &[2, 0, 0]);
    if with_table {
        section(&mut module, 4, &[1, 0x70, 0, 1]);
    }
    let mut globals = vec![1, 0x70, 1];
    globals.extend_from_slice(global_init);
    globals.push(0x0b);
    section(&mut module, 6, &globals);
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 1]);

    let target = [0, 0x41, 42, 0x0b];
    let mut run = vec![0];
    run.extend_from_slice(body_ops);
    run.push(0x0b);
    let mut code = vec![2, target.len() as u8];
    code.extend_from_slice(&target);
    code.push(run.len() as u8);
    code.extend_from_slice(&run);
    section(&mut module, 10, &code);
    module
}

#[test]
fn ref_func_global_get_is_non_null() {
    let module = parse_module(&module(&[0xd2, 0], &[0x23, 0, 0xd1], false)).unwrap();
    let mut instance = Instance::new(module).unwrap();
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(0))
    );
}

#[test]
fn mutable_funcref_global_accepts_ref_null() {
    let module = parse_module(&module(
        &[0xd2, 0],
        &[0xd0, 0x70, 0x24, 0, 0x23, 0, 0xd1],
        false,
    ))
    .unwrap();
    let mut instance = Instance::new(module).unwrap();
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(1))
    );
}

#[test]
fn null_funcref_global_initializer_executes() {
    let module = parse_module(&module(&[0xd0, 0x70], &[0x23, 0, 0xd1], false)).unwrap();
    let mut instance = Instance::new(module).unwrap();
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(1))
    );
}

#[test]
fn funcref_global_drives_table_set_and_call_indirect() {
    let module = parse_module(&module(
        &[0xd2, 0],
        &[0x41, 0, 0x23, 0, 0x26, 0, 0x41, 0, 0x11, 0, 0],
        true,
    ))
    .unwrap();
    let mut instance = Instance::new(module).unwrap();
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(42))
    );
}

#[test]
fn global_ref_func_rejects_missing_function() {
    let module = parse_module(&module(&[0xd2, 2], &[0x41, 0], false)).unwrap();
    assert!(matches!(
        Instance::new(module),
        Err(RuntimeError::Validation(
            ValidationError::GlobalFunctionRefOutOfBounds {
                global: 0,
                function_index: 2
            }
        ))
    ));
}
