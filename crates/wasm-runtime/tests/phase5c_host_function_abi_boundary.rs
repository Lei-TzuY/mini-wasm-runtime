use wasm_parser::{Export, ExportKind, FuncType, Import, ImportDesc, Module, ValueType};
use wasm_runtime::{
    HostCapabilities, HostRegistry, HostRegistryError, Instance, RuntimeError, Value,
};
use wasm_validator::{validate, ValidationError};

fn imported_function_module(params: Vec<ValueType>, results: Vec<ValueType>) -> Module {
    Module {
        types: vec![FuncType { params, results }],
        imports: vec![Import {
            module: "env".into(),
            name: "host".into(),
            desc: ImportDesc::Function(0),
        }],
        ..Module::default()
    }
}

fn exported_import_module(params: Vec<ValueType>, results: Vec<ValueType>) -> Module {
    let mut module = imported_function_module(params, results);
    module.exports.push(Export {
        name: "run".into(),
        kind: ExportKind::Function,
        index: 0,
    });
    module
}

fn unsupported_numeric_types() -> [ValueType; 3] {
    [ValueType::I64, ValueType::F32, ValueType::F64]
}

#[test]
fn validator_rejects_non_i32_host_function_parameters() {
    for value_type in unsupported_numeric_types() {
        let module = imported_function_module(vec![value_type], vec![]);
        assert_eq!(
            validate(&module),
            Err(ValidationError::UnsupportedImportValueType {
                import: 0,
                value_type,
            }),
            "host import parameter {value_type:?} must remain fail-closed until the mixed-numeric ABI slice is complete"
        );
    }
}

#[test]
fn validator_rejects_non_i32_host_function_results() {
    for value_type in unsupported_numeric_types() {
        let module = imported_function_module(vec![], vec![value_type]);
        assert_eq!(
            validate(&module),
            Err(ValidationError::UnsupportedImportValueType {
                import: 0,
                value_type,
            }),
            "host import result {value_type:?} must remain fail-closed until the mixed-numeric ABI slice is complete"
        );
    }
}

#[test]
fn validator_rejects_multiple_i32_host_function_results() {
    let module = imported_function_module(vec![], vec![ValueType::I32, ValueType::I32]);
    assert_eq!(
        validate(&module),
        Err(ValidationError::UnsupportedImportResultArity {
            import: 0,
            results: 2,
        })
    );
}

#[test]
fn registry_rejects_non_i32_host_function_parameters() {
    for value_type in unsupported_numeric_types() {
        let mut hosts = HostRegistry::new();
        let error = hosts
            .register(
                "env",
                "host",
                vec![value_type],
                vec![],
                HostCapabilities::NONE,
                |_context, _args| Ok(None),
            )
            .expect_err("non-i32 host parameters must not be admitted by the registry yet");
        assert!(matches!(error, HostRegistryError::UnsupportedSignature));
    }
}

#[test]
fn registry_rejects_non_i32_host_function_results() {
    for value_type in unsupported_numeric_types() {
        let mut hosts = HostRegistry::new();
        let error = hosts
            .register(
                "env",
                "host",
                vec![],
                vec![value_type],
                HostCapabilities::NONE,
                |_context, _args| Ok(None),
            )
            .expect_err("non-i32 host results must not be admitted by the registry yet");
        assert!(matches!(error, HostRegistryError::UnsupportedSignature));
    }
}

#[test]
fn registry_rejects_multiple_i32_host_function_results() {
    let mut hosts = HostRegistry::new();
    let error = hosts
        .register(
            "env",
            "host",
            vec![],
            vec![ValueType::I32, ValueType::I32],
            HostCapabilities::NONE,
            |_context, _args| Ok(None),
        )
        .expect_err("multi-result host callbacks must remain outside the current ABI");
    assert!(matches!(error, HostRegistryError::UnsupportedSignature));
}

#[test]
fn host_callback_missing_declared_result_fails_closed() {
    let module = exported_import_module(vec![], vec![ValueType::I32]);
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "host",
            vec![],
            vec![ValueType::I32],
            HostCapabilities::NONE,
            |_context, _args| Ok(None),
        )
        .expect("i32 host signature remains admitted");

    let mut instance = Instance::with_hosts(module, hosts).expect("host binding is valid");
    let error = instance
        .invoke_export("run", &[])
        .expect_err("missing declared host result must fail closed");
    assert!(matches!(
        error,
        RuntimeError::HostResultArityMismatch {
            expected: 1,
            actual: 0,
            ..
        }
    ));
}

#[test]
fn host_callback_unexpected_result_fails_closed() {
    let module = exported_import_module(vec![], vec![]);
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "host",
            vec![],
            vec![],
            HostCapabilities::NONE,
            |_context, _args| Ok(Some(Value::I32(7))),
        )
        .expect("zero-result i32 host signature remains admitted");

    let mut instance = Instance::with_hosts(module, hosts).expect("host binding is valid");
    let error = instance
        .invoke_export("run", &[])
        .expect_err("unexpected host result must fail closed");
    assert!(matches!(
        error,
        RuntimeError::HostResultArityMismatch {
            expected: 0,
            actual: 1,
            ..
        }
    ));
}

#[test]
fn host_callback_wrong_result_type_fails_closed() {
    let module = exported_import_module(vec![], vec![ValueType::I32]);
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "host",
            vec![],
            vec![ValueType::I32],
            HostCapabilities::NONE,
            |_context, _args| Ok(Some(Value::I64(7))),
        )
        .expect("i32 host signature remains admitted");

    let mut instance = Instance::with_hosts(module, hosts).expect("host binding is valid");
    let error = instance
        .invoke_export("run", &[])
        .expect_err("host callback result type must match its declared result type");
    assert!(matches!(
        error,
        RuntimeError::ValueTypeMismatch {
            expected: ValueType::I32,
            actual: ValueType::I64,
        }
    ));
}
