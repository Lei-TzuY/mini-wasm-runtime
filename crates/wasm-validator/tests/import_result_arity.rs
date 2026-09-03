use wasm_parser::{FuncType, Import, ImportDesc, Module, ValueType};
use wasm_validator::{validate, ValidationError};

#[test]
fn rejects_multi_result_function_import_at_validation_boundary() {
    let module = Module {
        types: vec![FuncType {
            params: vec![],
            results: vec![ValueType::I32, ValueType::I64],
        }],
        imports: vec![Import {
            module: "env".into(),
            name: "pair".into(),
            desc: ImportDesc::Function(0),
        }],
        ..Module::default()
    };

    assert_eq!(
        validate(&module),
        Err(ValidationError::UnsupportedImportResultArity {
            import: 0,
            results: 2,
        })
    );
}
