use wasm_parser::parse_module;
use wasm_runtime::{ExternValue, Instance, RuntimeError, Value};

fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    assert!(payload.len() < 128);
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

fn owned_funcref_module() -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();

    section(
        &mut module,
        1,
        &[3, 0x60, 0, 0, 0x60, 0, 1, 0x70, 0x60, 1, 0x70, 1, 0x70],
    );
    section(&mut module, 3, &[3, 0, 1, 2]);

    let mut exports = vec![2];
    for (name, index) in [(b"make".as_slice(), 1u8), (b"identity".as_slice(), 2u8)] {
        exports.push(name.len() as u8);
        exports.extend_from_slice(name);
        exports.push(0);
        exports.push(index);
    }
    section(&mut module, 7, &exports);

    let target = [0, 0x0b];
    let make = [0, 0xd2, 0, 0x0b];
    let identity = [0, 0x20, 0, 0x0b];
    let mut code = vec![3];
    for body in [target.as_slice(), make.as_slice(), identity.as_slice()] {
        code.push(body.len() as u8);
        code.extend_from_slice(body);
    }
    section(&mut module, 10, &code);
    module
}

fn new_instance() -> Instance {
    let module = parse_module(&owned_funcref_module()).expect("owned funcref module parses");
    Instance::new(module).expect("owned funcref module instantiates")
}

fn make_reference(instance: &mut Instance) -> wasm_runtime::FunctionRef {
    let values = instance
        .invoke_export_extern_values("make", &[])
        .expect("owned reference result crosses embedding boundary");
    let [ExternValue::FuncRef(Some(reference))] = values.as_slice() else {
        panic!("make must return one non-null owned funcref: {values:?}");
    };
    reference.clone()
}

#[test]
fn owned_funcref_roundtrips_through_its_instance() {
    let mut instance = new_instance();
    let reference = make_reference(&mut instance);

    assert_eq!(
        instance
            .invoke_export_extern_values(
                "identity",
                &[ExternValue::FuncRef(Some(reference.clone()))],
            )
            .unwrap(),
        vec![ExternValue::FuncRef(Some(reference))]
    );
    assert_eq!(
        instance
            .invoke_export_extern_values("identity", &[ExternValue::FuncRef(None)])
            .unwrap(),
        vec![ExternValue::FuncRef(None)]
    );
}

#[test]
fn foreign_and_expired_funcref_handles_fail_closed() {
    let mut owner = new_instance();
    let reference = make_reference(&mut owner);
    let mut foreign = new_instance();

    assert!(matches!(
        foreign.invoke_export_extern_values(
            "identity",
            &[ExternValue::FuncRef(Some(reference.clone()))],
        ),
        Err(RuntimeError::ForeignFunctionReferenceArgument)
    ));

    let stale = {
        let mut temporary_owner = new_instance();
        make_reference(&mut temporary_owner)
    };
    assert!(matches!(
        foreign.invoke_export_extern_values("identity", &[ExternValue::FuncRef(Some(stale))]),
        Err(RuntimeError::ExpiredFunctionReferenceArgument)
    ));
}

#[test]
fn legacy_raw_non_null_funcref_boundary_remains_fail_closed() {
    let mut instance = new_instance();
    assert!(matches!(
        instance.invoke_export_values("identity", &[Value::FuncRef(Some(0))]),
        Err(RuntimeError::UnownedFunctionReferenceArgument)
    ));
}
