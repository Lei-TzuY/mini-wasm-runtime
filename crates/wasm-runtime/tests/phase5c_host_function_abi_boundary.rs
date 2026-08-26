use wasm_parser::{FuncType, Import, ImportDesc, Module, ValueType};
use wasm_runtime::{HostCapabilities, HostRegistry, HostRegistryError};
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
