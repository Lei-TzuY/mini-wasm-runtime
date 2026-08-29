use std::{cell::Cell, rc::Rc};

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

fn non_i32_numeric_types() -> [ValueType; 3] {
    [ValueType::I64, ValueType::F32, ValueType::F64]
}

#[test]
fn validator_accepts_non_i32_host_function_parameters() {
    for value_type in non_i32_numeric_types() {
        let module = imported_function_module(vec![value_type], vec![]);
        assert_eq!(
            validate(&module),
            Ok(()),
            "numeric host import parameter {value_type:?} should be admitted by validation"
        );
    }
}

#[test]
fn validator_accepts_non_i32_host_function_results() {
    for value_type in non_i32_numeric_types() {
        let module = imported_function_module(vec![], vec![value_type]);
        assert_eq!(
            validate(&module),
            Ok(()),
            "numeric host import result {value_type:?} should be admitted by validation"
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
fn registry_accepts_non_i32_host_function_parameters() {
    for value_type in non_i32_numeric_types() {
        let mut hosts = HostRegistry::new();
        hosts
            .register(
                "env",
                "host",
                vec![value_type],
                vec![],
                HostCapabilities::NONE,
                |_context, _args| Ok(None),
            )
            .expect("all MVP numeric host parameter types should be admitted");
    }
}

#[test]
fn registry_accepts_non_i32_host_function_results() {
    for value_type in non_i32_numeric_types() {
        let mut hosts = HostRegistry::new();
        hosts
            .register(
                "env",
                "host",
                vec![],
                vec![value_type],
                HostCapabilities::NONE,
                |_context, _args| Ok(None),
            )
            .expect("all MVP numeric host result types should be admitted");
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
fn mixed_numeric_host_round_trip_preserves_exact_values() {
    let f32_bits = 0x7fc0_1234;
    let f64_bits = 0x7ff8_0000_0000_5678;
    let module = exported_import_module(
        vec![ValueType::I64, ValueType::F32, ValueType::F64],
        vec![ValueType::F64],
    );
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "host",
            vec![ValueType::I64, ValueType::F32, ValueType::F64],
            vec![ValueType::F64],
            HostCapabilities::NONE,
            move |_context, args| {
                assert_eq!(args[0].as_i64(), -9);
                assert_eq!(args[1].as_f32().to_bits(), f32_bits);
                assert_eq!(args[2].as_f64().to_bits(), f64_bits);
                Ok(Some(args[2]))
            },
        )
        .expect("mixed numeric host signature should be admitted");

    let mut instance = Instance::with_hosts(module, hosts).expect("host binding is valid");
    let result = instance
        .invoke_export(
            "run",
            &[
                Value::I64(-9),
                Value::F32(f32::from_bits(f32_bits)),
                Value::F64(f64::from_bits(f64_bits)),
            ],
        )
        .expect("mixed numeric host call should execute");
    let Some(Value::F64(value)) = result else {
        panic!("mixed numeric host call should return f64");
    };
    assert_eq!(value.to_bits(), f64_bits);
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
fn host_callback_wrong_result_type_fails_closed_for_every_other_numeric_type() {
    for (value, actual) in [
        (Value::I64(7), ValueType::I64),
        (Value::F32(f32::from_bits(0x7fc0_0001)), ValueType::F32),
        (
            Value::F64(f64::from_bits(0x7ff8_0000_0000_0001)),
            ValueType::F64,
        ),
    ] {
        let module = exported_import_module(vec![], vec![ValueType::I32]);
        let mut hosts = HostRegistry::new();
        hosts
            .register(
                "env",
                "host",
                vec![],
                vec![ValueType::I32],
                HostCapabilities::NONE,
                move |_context, _args| Ok(Some(value)),
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
                actual: observed,
            } if observed == actual
        ));
    }
}

#[test]
fn host_callback_is_not_invoked_when_argument_type_mismatches() {
    for (value, actual) in [
        (Value::I64(7), ValueType::I64),
        (Value::F32(f32::from_bits(0x7fc0_0001)), ValueType::F32),
        (
            Value::F64(f64::from_bits(0x7ff8_0000_0000_0001)),
            ValueType::F64,
        ),
    ] {
        let module = exported_import_module(vec![ValueType::I32], vec![]);
        let callback_called = Rc::new(Cell::new(false));
        let callback_called_from_host = Rc::clone(&callback_called);
        let mut hosts = HostRegistry::new();
        hosts
            .register(
                "env",
                "host",
                vec![ValueType::I32],
                vec![],
                HostCapabilities::NONE,
                move |_context, _args| {
                    callback_called_from_host.set(true);
                    Ok(None)
                },
            )
            .expect("i32 host signature remains admitted");

        let mut instance = Instance::with_hosts(module, hosts).expect("host binding is valid");
        let error = instance
            .invoke_export("run", &[value])
            .expect_err("mismatched host argument type must fail before callback execution");
        assert!(matches!(
            error,
            RuntimeError::ValueTypeMismatch {
                expected: ValueType::I32,
                actual: observed,
            } if observed == actual
        ));
        assert!(
            !callback_called.get(),
            "host callback must not observe arguments rejected by the declared ABI"
        );
    }
}
