from pathlib import Path


def replace_once(path: str, old: str, new: str, label: str) -> None:
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one anchor, found {count}")
    p.write_text(text.replace(old, new, 1))


runtime = "crates/wasm-runtime/src/lib.rs"
validator = "crates/wasm-validator/src/lib.rs"

replace_once(
    validator,
    "//! Function imports remain i32-only while defined code may use i32/i64/f32/f64.\n",
    "//! Function imports and defined code may use i32/i64/f32/f64 with at most one result.\n",
    "validator module docs",
)

replace_once(
    validator,
    '''    UnsupportedImportValueType {
        import: usize,
        value_type: ValueType,
    },
''',
    "",
    "remove obsolete import value type error",
)

replace_once(
    validator,
    '''            Self::UnsupportedImportValueType { import, value_type } => write!(
                f,
                "function import {import} uses {value_type:?}; host calls are currently i32-only"
            ),
''',
    "",
    "remove obsolete import error display",
)

replace_once(
    validator,
    '''                for &value_type in function_type
                    .params
                    .iter()
                    .chain(function_type.results.iter())
                {
                    if value_type != ValueType::I32 {
                        return Err(ValidationError::UnsupportedImportValueType {
                            import,
                            value_type,
                        });
                    }
                }
''',
    "",
    "remove i32-only import validation",
)

replace_once(
    validator,
    '''    fn rejects_non_i32_import_signature() {
        let module = Module {
            types: vec![FuncType {
                params: vec![ValueType::I64],
                results: vec![],
            }],
            imports: vec![import("env", "f", 0)],
            ..Module::default()
        };
        assert_eq!(
            validate(&module),
            Err(ValidationError::UnsupportedImportValueType {
                import: 0,
                value_type: ValueType::I64,
            })
        );
    }
''',
    '''    fn accepts_all_numeric_import_signature_types() {
        let module = Module {
            types: vec![FuncType {
                params: vec![
                    ValueType::I32,
                    ValueType::I64,
                    ValueType::F32,
                    ValueType::F64,
                ],
                results: vec![ValueType::F64],
            }],
            imports: vec![import("env", "f", 0)],
            ..Module::default()
        };
        assert_eq!(validate(&module), Ok(()));
    }
''',
    "replace legacy non-i32 validator test",
)

replace_once(
    runtime,
    '''            Self::UnsupportedSignature => write!(
                f,
                "host function signatures are currently i32-only with at most one result"
            ),
''',
    '''            Self::UnsupportedSignature => write!(
                f,
                "host function signatures support numeric value types with at most one result"
            ),
''',
    "host registry error message",
)

replace_once(
    runtime,
    '''        if results.len() > 1
            || params
                .iter()
                .chain(results.iter())
                .any(|ty| *ty != ValueType::I32)
        {
            return Err(HostRegistryError::UnsupportedSignature);
        }
''',
    '''        if results.len() > 1 {
            return Err(HostRegistryError::UnsupportedSignature);
        }
''',
    "remove i32-only host registration gate",
)

replace_once(
    runtime,
    '''    HostResultArityMismatch {
        module: String,
        name: String,
        expected: usize,
        actual: usize,
    },
''',
    '''    HostResultArityMismatch {
        module: String,
        name: String,
        expected: usize,
        actual: usize,
    },
    HostResultTypeMismatch {
        module: String,
        name: String,
        expected: ValueType,
        actual: ValueType,
    },
''',
    "runtime host result type error",
)

replace_once(
    runtime,
    '''            Self::HostResultArityMismatch {
                module,
                name,
                expected,
                actual,
            } => write!(
                f,
                "host function {module}.{name} returned {actual} values, expected {expected}"
            ),
''',
    '''            Self::HostResultArityMismatch {
                module,
                name,
                expected,
                actual,
            } => write!(
                f,
                "host function {module}.{name} returned {actual} values, expected {expected}"
            ),
            Self::HostResultTypeMismatch {
                module,
                name,
                expected,
                actual,
            } => write!(
                f,
                "host function {module}.{name} returned {actual:?}, expected {expected:?}"
            ),
''',
    "runtime host result type display",
)

replace_once(
    runtime,
    '''        if actual != ty.results.len() {
            return Err(RuntimeError::HostResultArityMismatch {
                module: import.module,
                name: import.name,
                expected: ty.results.len(),
                actual,
            });
        }
        Ok(result)
''',
    '''        if actual != ty.results.len() {
            return Err(RuntimeError::HostResultArityMismatch {
                module: import.module,
                name: import.name,
                expected: ty.results.len(),
                actual,
            });
        }
        if let (Some(value), Some(&expected)) = (result, ty.results.first()) {
            let actual = value.value_type();
            if actual != expected {
                return Err(RuntimeError::HostResultTypeMismatch {
                    module: import.module,
                    name: import.name,
                    expected,
                    actual,
                });
            }
        }
        Ok(result)
''',
    "host result runtime type defense",
)


test = r'''use std::{cell::Cell, rc::Rc};

use wasm_parser::{Export, ExportKind, FuncType, FunctionBody, Import, ImportDesc, Module, ValueType};
use wasm_runtime::{
    HostCapabilities, HostRegistry, HostRegistryError, Instance, RuntimeError, Value,
};

fn imported_host_module(params: Vec<ValueType>, results: Vec<ValueType>) -> Module {
    Module {
        types: vec![FuncType { params, results }],
        imports: vec![Import {
            module: "env".into(),
            name: "host".into(),
            desc: ImportDesc::Function(0),
        }],
        exports: vec![Export {
            name: "host".into(),
            kind: ExportKind::Function,
            index: 0,
        }],
        ..Module::default()
    }
}

#[test]
fn i64_host_import_round_trips_typed_value() {
    let module = imported_host_module(vec![ValueType::I64], vec![ValueType::I64]);
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "host",
            vec![ValueType::I64],
            vec![ValueType::I64],
            HostCapabilities::NONE,
            |_ctx, args| Ok(Some(Value::I64(args[0].as_i64().wrapping_add(1)))),
        )
        .unwrap();
    let mut vm = Instance::with_hosts(module, hosts).unwrap();
    assert_eq!(
        vm.invoke_export("host", &[Value::I64(41)]).unwrap(),
        Some(Value::I64(42))
    );
}

#[test]
fn mixed_numeric_host_parameters_arrive_in_order_and_type() {
    let params = vec![
        ValueType::I32,
        ValueType::I64,
        ValueType::F32,
        ValueType::F64,
    ];
    let module = imported_host_module(params.clone(), vec![ValueType::I64]);
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "host",
            params,
            vec![ValueType::I64],
            HostCapabilities::NONE,
            |_ctx, args| {
                assert_eq!(args[0].as_i32(), 7);
                assert_eq!(args[1].as_i64(), 9);
                assert_eq!(args[2].as_f32().to_bits(), 1.5f32.to_bits());
                assert_eq!(args[3].as_f64().to_bits(), (-2.25f64).to_bits());
                Ok(Some(Value::I64(99)))
            },
        )
        .unwrap();
    let mut vm = Instance::with_hosts(module, hosts).unwrap();
    assert_eq!(
        vm.invoke_export(
            "host",
            &[
                Value::I32(7),
                Value::I64(9),
                Value::F32(1.5),
                Value::F64(-2.25),
            ],
        )
        .unwrap(),
        Some(Value::I64(99))
    );
}

#[test]
fn floating_host_results_preserve_nan_payload_bits() {
    for (result_type, expected, callback_value) in [
        (
            ValueType::F32,
            0x7fc1_2345u64,
            Value::F32(f32::from_bits(0x7fc1_2345)),
        ),
        (
            ValueType::F64,
            0x7ff8_0000_dead_beefu64,
            Value::F64(f64::from_bits(0x7ff8_0000_dead_beef)),
        ),
    ] {
        let module = imported_host_module(vec![], vec![result_type]);
        let mut hosts = HostRegistry::new();
        hosts
            .register(
                "env",
                "host",
                vec![],
                vec![result_type],
                HostCapabilities::NONE,
                move |_ctx, _args| Ok(Some(callback_value)),
            )
            .unwrap();
        let mut vm = Instance::with_hosts(module, hosts).unwrap();
        let value = vm.invoke_export("host", &[]).unwrap().unwrap();
        let actual = match value {
            Value::F32(value) => u64::from(value.to_bits()),
            Value::F64(value) => value.to_bits(),
            other => panic!("unexpected host result type: {:?}", other.value_type()),
        };
        assert_eq!(actual, expected);
    }
}

#[test]
fn wrong_non_i32_argument_variant_is_rejected_before_callback() {
    let module = imported_host_module(vec![ValueType::F64], vec![]);
    let called = Rc::new(Cell::new(false));
    let callback_called = called.clone();
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "host",
            vec![ValueType::F64],
            vec![],
            HostCapabilities::NONE,
            move |_ctx, _args| {
                callback_called.set(true);
                Ok(None)
            },
        )
        .unwrap();
    let mut vm = Instance::with_hosts(module, hosts).unwrap();
    assert!(matches!(
        vm.invoke_export("host", &[Value::I64(1)]),
        Err(RuntimeError::ValueTypeMismatch {
            expected: ValueType::F64,
            actual: ValueType::I64,
        })
    ));
    assert!(!called.get());
}

#[test]
fn wrong_host_result_variant_is_rejected_at_host_boundary() {
    let module = imported_host_module(vec![], vec![ValueType::F64]);
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "host",
            vec![],
            vec![ValueType::F64],
            HostCapabilities::NONE,
            |_ctx, _args| Ok(Some(Value::I64(7))),
        )
        .unwrap();
    let mut vm = Instance::with_hosts(module, hosts).unwrap();
    assert!(matches!(
        vm.invoke_export("host", &[]),
        Err(RuntimeError::HostResultTypeMismatch {
            expected: ValueType::F64,
            actual: ValueType::I64,
            ..
        })
    ));
}

#[test]
fn host_registration_still_rejects_multi_value_results() {
    let mut hosts = HostRegistry::new();
    assert_eq!(
        hosts.register(
            "env",
            "host",
            vec![ValueType::F64],
            vec![ValueType::I64, ValueType::F64],
            HostCapabilities::NONE,
            |_ctx, _args| Ok(None),
        ),
        Err(HostRegistryError::UnsupportedSignature)
    );
}

#[test]
fn non_i32_host_signature_mismatch_fails_instantiation() {
    let module = imported_host_module(vec![ValueType::F64], vec![ValueType::F64]);
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "host",
            vec![ValueType::F32],
            vec![ValueType::F64],
            HostCapabilities::NONE,
            |_ctx, _args| Ok(Some(Value::F64(0.0))),
        )
        .unwrap();
    assert!(matches!(
        Instance::with_hosts(module, hosts),
        Err(RuntimeError::HostSignatureMismatch { .. })
    ));
}

#[test]
fn defined_wasm_code_consumes_typed_host_result() {
    let ty = FuncType {
        params: vec![ValueType::I64],
        results: vec![ValueType::I64],
    };
    let module = Module {
        types: vec![ty.clone()],
        imports: vec![Import {
            module: "env".into(),
            name: "host".into(),
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
            code: vec![0x20, 0x00, 0x10, 0x00, 0x42, 0x02, 0x7e, 0x0b],
        }],
        ..Module::default()
    };
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "host",
            ty.params.clone(),
            ty.results.clone(),
            HostCapabilities::NONE,
            |_ctx, args| Ok(Some(Value::I64(args[0].as_i64().wrapping_add(1)))),
        )
        .unwrap();
    let mut vm = Instance::with_hosts(module, hosts).unwrap();
    assert_eq!(
        vm.invoke_export("run", &[Value::I64(20)]).unwrap(),
        Some(Value::I64(42))
    );
}
'''
Path("crates/wasm-runtime/tests/phase5c_typed_host_abi.rs").write_text(test)
