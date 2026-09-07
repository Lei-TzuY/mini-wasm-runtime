use wasm_parser::{parse_module, ValueType};
use wasm_runtime::{
    GlobalHandle, HostCapabilities, HostRegistry, HostRegistryError, Instance, RuntimeError, Value,
};

fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    assert!(payload.len() < 128);
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

fn reference_function_module() -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();

    section(
        &mut module,
        1,
        &[3, 0x60, 0, 0, 0x60, 1, 0x70, 1, 0x70, 0x60, 0, 1, 0x7f],
    );
    section(&mut module, 3, &[5, 0, 1, 2, 2, 2]);

    let mut exports = vec![5];
    for (name, index) in [
        (b"target".as_slice(), 0u8),
        (b"identity".as_slice(), 1u8),
        (b"roundtrip".as_slice(), 2u8),
        (b"default_local".as_slice(), 3u8),
        (b"non_null".as_slice(), 4u8),
    ] {
        exports.push(name.len() as u8);
        exports.extend_from_slice(name);
        exports.push(0);
        exports.push(index);
    }
    section(&mut module, 7, &exports);

    let target = [0, 0x0b];
    let identity = [0, 0x20, 0, 0x0b];
    let roundtrip = [0, 0xd0, 0x70, 0x10, 1, 0xd1, 0x0b];
    let default_local = [1, 1, 0x70, 0x20, 0, 0xd1, 0x0b];
    let non_null = [1, 1, 0x70, 0xd2, 0, 0x21, 0, 0x20, 0, 0xd1, 0x0b];
    let mut code = vec![5];
    for body in [
        target.as_slice(),
        identity.as_slice(),
        roundtrip.as_slice(),
        default_local.as_slice(),
        non_null.as_slice(),
    ] {
        code.push(body.len() as u8);
        code.extend_from_slice(body);
    }
    section(&mut module, 10, &code);
    module
}

#[test]
fn defined_calls_and_locals_preserve_funcref_values() {
    let module = parse_module(&reference_function_module()).expect("reference module parses");
    let mut instance = Instance::new(module).expect("reference module instantiates");

    for (export, expected) in [("roundtrip", 1), ("default_local", 1), ("non_null", 0)] {
        assert_eq!(
            instance.invoke_export_values(export, &[]).unwrap(),
            vec![Value::I32(expected)]
        );
    }
}

#[test]
fn embedding_boundary_accepts_null_but_rejects_unowned_non_null_funcref() {
    let module = parse_module(&reference_function_module()).expect("reference module parses");
    let mut instance = Instance::new(module).expect("reference module instantiates");

    assert_eq!(
        instance
            .invoke_export_values("identity", &[Value::FuncRef(None)])
            .unwrap(),
        vec![Value::FuncRef(None)]
    );
    assert!(matches!(
        instance.invoke_export_values("identity", &[Value::FuncRef(Some(0))]),
        Err(RuntimeError::UnownedFunctionReferenceArgument)
    ));
    assert!(matches!(
        instance.invoke_export("identity", &[Value::FuncRef(Some(0))]),
        Err(RuntimeError::UnownedFunctionReferenceArgument)
    ));
}

#[test]
fn host_registry_rejects_reference_typed_bindings() {
    let mut hosts = HostRegistry::new();
    assert_eq!(
        hosts.register_values(
            "host",
            "ref_id",
            vec![ValueType::FuncRef],
            vec![ValueType::FuncRef],
            HostCapabilities::NONE,
            |_context, args| Ok(args.to_vec()),
        ),
        Err(HostRegistryError::UnsupportedSignature)
    );
    assert_eq!(
        hosts.register_global(
            "host",
            "ref_global",
            GlobalHandle::immutable(Value::FuncRef(None)),
        ),
        Err(HostRegistryError::UnsupportedSignature)
    );
}
