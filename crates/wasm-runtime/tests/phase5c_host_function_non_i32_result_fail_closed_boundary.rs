use wasm_parser::{Export, ExportKind, FuncType, Import, ImportDesc, Module, ValueType};
use wasm_runtime::{HostCapabilities, HostRegistry, Instance, RuntimeError, Value};

fn exported_import_module(result: ValueType) -> Module {
    Module {
        types: vec![FuncType {
            params: vec![],
            results: vec![result],
        }],
        imports: vec![Import {
            module: "env".into(),
            name: "host".into(),
            desc: ImportDesc::Function(0),
        }],
        exports: vec![Export {
            name: "run".into(),
            kind: ExportKind::Function,
            index: 0,
        }],
        ..Module::default()
    }
}

#[test]
fn non_i32_host_callback_result_types_fail_closed_on_mismatch() {
    for (expected, value, actual) in [
        (
            ValueType::I64,
            Value::F32(f32::from_bits(0x7fc0_1234)),
            ValueType::F32,
        ),
        (
            ValueType::F32,
            Value::F64(f64::from_bits(0x7ff8_0000_0000_5678)),
            ValueType::F64,
        ),
        (ValueType::F64, Value::I64(-9), ValueType::I64),
    ] {
        let module = exported_import_module(expected);
        let mut hosts = HostRegistry::new();
        hosts
            .register(
                "env",
                "host",
                vec![],
                vec![expected],
                HostCapabilities::NONE,
                move |_context, _args| Ok(Some(value)),
            )
            .expect("all single-result MVP numeric host signatures should be admitted");

        let mut instance = Instance::with_hosts(module, hosts).expect("host binding is valid");
        let error = instance
            .invoke_export("run", &[])
            .expect_err("host callback result must match its declared non-i32 result type");
        assert!(matches!(
            error,
            RuntimeError::ValueTypeMismatch {
                expected: observed_expected,
                actual: observed_actual,
            } if observed_expected == expected && observed_actual == actual
        ));
    }
}
