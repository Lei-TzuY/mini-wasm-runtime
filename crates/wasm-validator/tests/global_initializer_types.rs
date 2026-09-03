use wasm_parser::{Constant, Global, GlobalType, Import, ImportDesc, Module, ValueType};
use wasm_validator::{validate, ValidationError};

#[test]
fn rejects_defined_global_initializer_type_mismatch() {
    let module = Module {
        imports: vec![Import {
            module: "env".into(),
            name: "imported".into(),
            desc: ImportDesc::Global(GlobalType {
                value_type: ValueType::I32,
                mutable: false,
            }),
        }],
        globals: vec![Global {
            ty: GlobalType {
                value_type: ValueType::I32,
                mutable: false,
            },
            init: Constant::I64(7),
        }],
        ..Module::default()
    };

    assert_eq!(
        validate(&module),
        Err(ValidationError::GlobalInitializerTypeMismatch {
            global: 1,
            expected: ValueType::I32,
            actual: ValueType::I64,
        })
    );
}
