use std::{cell::Cell, rc::Rc};

use wasm_parser::{
    Export, ExportKind, FuncType, FunctionBody, Import, ImportDesc, Module, ValueType,
};
use wasm_runtime::{
    HostCapabilities, HostRegistry, HostRegistryError, Instance, RuntimeError, Value,
};

fn forwarding_multi_result_import() -> Module {
    Module {
        types: vec![FuncType {
            params: vec![ValueType::I32],
            results: vec![
                ValueType::I32,
                ValueType::I64,
                ValueType::F32,
                ValueType::F64,
            ],
        }],
        imports: vec![Import {
            module: "env".into(),
            name: "multi".into(),
            desc: ImportDesc::Function(0),
        }],
        function_type_indices: vec![0],
        exports: vec![Export {
            name: "run".into(),
            kind: ExportKind::Function,
            index: 1,
        }],
        code: vec![FunctionBody {
            locals: vec![],
            code: vec![0x20, 0x00, 0x10, 0x00, 0x0b],
        }],
        ..Module::default()
    }
}

fn multi_result_types() -> Vec<ValueType> {
    vec![
        ValueType::I32,
        ValueType::I64,
        ValueType::F32,
        ValueType::F64,
    ]
}

#[test]
fn vector_host_callback_forwards_ordered_multi_results_through_defined_wasm() {
    let mut hosts = HostRegistry::new();
    hosts
        .register_values(
            "env",
            "multi",
            vec![ValueType::I32],
            multi_result_types(),
            HostCapabilities::NONE,
            |_ctx, args| {
                let input = args[0].as_i32();
                Ok(vec![
                    Value::I32(input.wrapping_add(1)),
                    Value::I64(i64::from(input).wrapping_mul(2)),
                    Value::F32(input as f32 + 0.5),
                    Value::F64(f64::from(input) - 0.25),
                ])
            },
        )
        .unwrap();

    let mut instance = Instance::with_hosts(forwarding_multi_result_import(), hosts).unwrap();
    let results = instance
        .invoke_export_values("run", &[Value::I32(17)])
        .unwrap();
    assert_eq!(results[0], Value::I32(18));
    assert_eq!(results[1], Value::I64(34));
    assert_eq!(results[2].as_f32().to_bits(), 17.5_f32.to_bits());
    assert_eq!(results[3].as_f64().to_bits(), 16.75_f64.to_bits());
}

#[test]
fn legacy_register_remains_zero_or_one_result_only() {
    let mut hosts = HostRegistry::new();
    assert_eq!(
        hosts.register(
            "env",
            "multi",
            vec![],
            vec![ValueType::I32, ValueType::I64],
            HostCapabilities::NONE,
            |_ctx, _args| Ok(Some(Value::I32(1))),
        ),
        Err(HostRegistryError::UnsupportedSignature)
    );
}

#[test]
fn legacy_invoke_api_rejects_multi_result_before_host_callback_runs() {
    let calls = Rc::new(Cell::new(0usize));
    let callback_calls = Rc::clone(&calls);
    let mut hosts = HostRegistry::new();
    hosts
        .register_values(
            "env",
            "multi",
            vec![ValueType::I32],
            multi_result_types(),
            HostCapabilities::NONE,
            move |_ctx, _args| {
                callback_calls.set(callback_calls.get() + 1);
                Ok(vec![
                    Value::I32(1),
                    Value::I64(2),
                    Value::F32(3.0),
                    Value::F64(4.0),
                ])
            },
        )
        .unwrap();
    let mut instance = Instance::with_hosts(forwarding_multi_result_import(), hosts).unwrap();

    assert!(matches!(
        instance.invoke_export("run", &[Value::I32(0)]),
        Err(RuntimeError::MultiValueResultRequiresValuesApi { results: 4 })
    ));
    assert_eq!(calls.get(), 0);
}

#[test]
fn vector_host_callback_result_arity_is_checked_after_callback() {
    let mut hosts = HostRegistry::new();
    hosts
        .register_values(
            "env",
            "multi",
            vec![ValueType::I32],
            multi_result_types(),
            HostCapabilities::NONE,
            |_ctx, _args| Ok(vec![Value::I32(1), Value::I64(2), Value::F32(3.0)]),
        )
        .unwrap();
    let mut instance = Instance::with_hosts(forwarding_multi_result_import(), hosts).unwrap();

    assert!(matches!(
        instance.invoke_export_values("run", &[Value::I32(0)]),
        Err(RuntimeError::HostResultArityMismatch {
            expected: 4,
            actual: 3,
            ..
        })
    ));
}

#[test]
fn vector_host_callback_result_types_are_checked_in_declared_order() {
    let mut hosts = HostRegistry::new();
    hosts
        .register_values(
            "env",
            "multi",
            vec![ValueType::I32],
            multi_result_types(),
            HostCapabilities::NONE,
            |_ctx, _args| {
                Ok(vec![
                    Value::I32(1),
                    Value::I64(2),
                    Value::F64(3.0),
                    Value::F32(4.0),
                ])
            },
        )
        .unwrap();
    let mut instance = Instance::with_hosts(forwarding_multi_result_import(), hosts).unwrap();

    assert!(matches!(
        instance.invoke_export_values("run", &[Value::I32(0)]),
        Err(RuntimeError::HostResultTypeMismatch {
            expected: ValueType::F32,
            actual: ValueType::F64,
            ..
        })
    ));
}

#[test]
fn vector_host_registration_still_requires_exact_import_signature() {
    let mut hosts = HostRegistry::new();
    hosts
        .register_values(
            "env",
            "multi",
            vec![ValueType::I32],
            vec![ValueType::I32, ValueType::I64],
            HostCapabilities::NONE,
            |_ctx, _args| Ok(vec![Value::I32(1), Value::I64(2)]),
        )
        .unwrap();

    assert!(matches!(
        Instance::with_hosts(forwarding_multi_result_import(), hosts),
        Err(RuntimeError::HostSignatureMismatch { .. })
    ));
}
