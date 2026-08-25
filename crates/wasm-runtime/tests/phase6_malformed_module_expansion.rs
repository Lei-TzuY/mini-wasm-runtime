use wasm_parser::{
    parse_module, DataMode, DataSegment, ElementMode, ElementSegment, Export, ExportKind, FuncType,
    GlobalType, Import, ImportDesc, Limits, MemoryType, Module, TableType, ValueType,
};
use wasm_runtime::{
    GlobalHandle, HostCapabilities, HostRegistry, Instance, MemoryHandle, RuntimeError,
    TableHandle, Value,
};
use wasm_validator::{validate, ValidationError, MAX_MEMORY_PAGES};

const I32: u8 = 0x7f;

fn push_u32(bytes: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        bytes.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn header() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

fn function_module(
    params: &[u8],
    results: &[u8],
    instructions: &[u8],
    memory_minimum: Option<u32>,
) -> Vec<u8> {
    let mut bytes = header();

    let mut ty = vec![0x01, 0x60];
    push_u32(&mut ty, params.len() as u32);
    ty.extend_from_slice(params);
    push_u32(&mut ty, results.len() as u32);
    ty.extend_from_slice(results);
    push_section(&mut bytes, 1, &ty);
    push_section(&mut bytes, 3, &[0x01, 0x00]);

    if let Some(minimum) = memory_minimum {
        let mut memory = vec![0x01, 0x00];
        push_u32(&mut memory, minimum);
        push_section(&mut bytes, 5, &memory);
    }

    push_section(&mut bytes, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);

    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend(body);
    push_section(&mut bytes, 10, &code);
    bytes
}

fn parsed(bytes: Vec<u8>) -> Module {
    parse_module(&bytes).expect("stage-corpus seed must parse before later-stage mutation")
}

fn empty_parsed() -> Module {
    parsed(header())
}

fn import_only(desc: ImportDesc, types: Vec<FuncType>) -> Module {
    let mut module = empty_parsed();
    module.types = types;
    module.imports.push(Import {
        module: "env".to_owned(),
        name: "item".to_owned(),
        desc,
    });
    module
}

fn expect_validation(
    module: &Module,
    predicate: impl FnOnce(&ValidationError) -> bool,
    name: &str,
) {
    let error = match validate(module) {
        Ok(()) => panic!("{name}: malformed module validated"),
        Err(error) => error,
    };
    assert!(
        predicate(&error),
        "{name}: unexpected validation error: {error:?}"
    );
}

fn expect_instantiation(
    module: Module,
    hosts: HostRegistry,
    predicate: impl FnOnce(&RuntimeError) -> bool,
    name: &str,
) {
    validate(&module).unwrap_or_else(|error| {
        panic!("{name}: fixture must survive validation before instantiation: {error:?}")
    });
    let error = Instance::with_hosts(module, hosts)
        .expect_err(&format!("{name}: fixture unexpectedly instantiated"));
    assert!(
        predicate(&error),
        "{name}: unexpected instantiation error: {error:?}"
    );
}

#[test]
fn expanded_validation_corpus_rejects_cross_index_and_segment_mutations() {
    let seed = || parsed(function_module(&[], &[], &[], None));

    let mut import_type_oob = seed();
    import_type_oob.imports.push(Import {
        module: "env".to_owned(),
        name: "f".to_owned(),
        desc: ImportDesc::Function(99),
    });
    expect_validation(
        &import_type_oob,
        |error| {
            matches!(
                error,
                ValidationError::ImportTypeIndexOutOfBounds {
                    import: 0,
                    type_index: 99
                }
            )
        },
        "function import type index out of bounds",
    );

    for (kind, name) in [
        (ExportKind::Memory, "memory"),
        (ExportKind::Table, "table"),
        (ExportKind::Global, "global"),
    ] {
        let mut module = seed();
        module.exports.clear();
        module.exports.push(Export {
            name: name.to_owned(),
            kind,
            index: 0,
        });
        expect_validation(
            &module,
            |error| match kind {
                ExportKind::Memory => matches!(
                    error,
                    ValidationError::MemoryExportOutOfBounds {
                        memory_index: 0,
                        ..
                    }
                ),
                ExportKind::Table => matches!(
                    error,
                    ValidationError::TableExportOutOfBounds { table_index: 0, .. }
                ),
                ExportKind::Global => matches!(
                    error,
                    ValidationError::GlobalExportOutOfBounds {
                        global_index: 0,
                        ..
                    }
                ),
                ExportKind::Function => false,
            },
            &format!("{name} export out of bounds"),
        );
    }

    let mut invalid_table_limits = seed();
    invalid_table_limits.tables.push(TableType {
        limits: Limits {
            min: 2,
            max: Some(1),
        },
    });
    expect_validation(
        &invalid_table_limits,
        |error| {
            matches!(
                error,
                ValidationError::InvalidTableLimits {
                    table: 0,
                    min: 2,
                    max: 1
                }
            )
        },
        "invalid table limits",
    );

    let mut memory_page_limit = seed();
    memory_page_limit.memories.push(MemoryType {
        limits: Limits {
            min: MAX_MEMORY_PAGES + 1,
            max: None,
        },
    });
    expect_validation(
        &memory_page_limit,
        |error| {
            matches!(
                error,
                ValidationError::MemoryPageLimitExceeded {
                    memory: 0,
                    pages
                } if *pages == MAX_MEMORY_PAGES + 1
            )
        },
        "memory page limit exceeded",
    );

    let mut element_table_oob = seed();
    element_table_oob.elements.push(ElementSegment {
        mode: ElementMode::Active {
            table_index: 0,
            offset: 0,
        },
        function_indices: vec![],
    });
    expect_validation(
        &element_table_oob,
        |error| {
            matches!(
                error,
                ValidationError::ElementTableOutOfBounds {
                    segment: 0,
                    table_index: 0
                }
            )
        },
        "element table out of bounds",
    );

    let mut element_function_oob = seed();
    element_function_oob.tables.push(TableType {
        limits: Limits {
            min: 1,
            max: Some(1),
        },
    });
    element_function_oob.elements.push(ElementSegment {
        mode: ElementMode::Active {
            table_index: 0,
            offset: 0,
        },
        function_indices: vec![1],
    });
    expect_validation(
        &element_function_oob,
        |error| {
            matches!(
                error,
                ValidationError::ElementFunctionOutOfBounds {
                    segment: 0,
                    function_index: 1
                }
            )
        },
        "element function out of bounds",
    );

    let mut data_memory_oob = seed();
    data_memory_oob.data.push(DataSegment {
        mode: DataMode::Active {
            memory_index: 0,
            offset: 0,
        },
        bytes: vec![0xaa],
    });
    expect_validation(
        &data_memory_oob,
        |error| {
            matches!(
                error,
                ValidationError::DataMemoryOutOfBounds {
                    segment: 0,
                    memory_index: 0
                }
            )
        },
        "data memory out of bounds",
    );

    let indirect_table_oob = parsed(function_module(
        &[],
        &[],
        &[0x41, 0x00, 0x11, 0x00, 0x01],
        None,
    ));
    expect_validation(
        &indirect_table_oob,
        |error| {
            matches!(
                error,
                ValidationError::TableIndexOutOfBounds { table_index: 1, .. }
            )
        },
        "call_indirect table index out of bounds",
    );

    let mut indirect_type_oob = parsed(function_module(
        &[],
        &[],
        &[0x41, 0x00, 0x11, 0x01, 0x00],
        None,
    ));
    indirect_type_oob.tables.push(TableType {
        limits: Limits {
            min: 1,
            max: Some(1),
        },
    });
    expect_validation(
        &indirect_type_oob,
        |error| {
            matches!(
                error,
                ValidationError::IndirectTypeIndexOutOfBounds { type_index: 1, .. }
            )
        },
        "call_indirect type index out of bounds",
    );

    let mut start_oob = seed();
    start_oob.start = Some(1);
    expect_validation(
        &start_oob,
        |error| {
            matches!(
                error,
                ValidationError::StartFunctionOutOfBounds { function_index: 1 }
            )
        },
        "start function out of bounds",
    );

    let mut missing_function_end = seed();
    missing_function_end.code[0].code.pop();
    expect_validation(
        &missing_function_end,
        |error| matches!(error, ValidationError::MissingFunctionEnd { function: 0 }),
        "missing function end after parsed-model mutation",
    );
}

#[test]
fn import_binding_failures_are_confined_to_instantiation() {
    let function_type = FuncType {
        params: vec![],
        results: vec![],
    };
    let function_import = import_only(ImportDesc::Function(0), vec![function_type.clone()]);
    expect_instantiation(
        function_import.clone(),
        HostRegistry::new(),
        |error| {
            matches!(
                error,
                RuntimeError::UnresolvedImport { module, name }
                    if module == "env" && name == "item"
            )
        },
        "unresolved function import",
    );

    let mut wrong_function_hosts = HostRegistry::new();
    wrong_function_hosts
        .register(
            "env",
            "item",
            vec![],
            vec![ValueType::I32],
            HostCapabilities::NONE,
            |_context, _args| Ok(Some(Value::I32(0))),
        )
        .unwrap();
    expect_instantiation(
        function_import,
        wrong_function_hosts,
        |error| {
            matches!(
                error,
                RuntimeError::HostSignatureMismatch { module, name }
                    if module == "env" && name == "item"
            )
        },
        "host function signature mismatch",
    );

    let immutable_i32 = GlobalType {
        value_type: ValueType::I32,
        mutable: false,
    };
    let global_import = import_only(ImportDesc::Global(immutable_i32), vec![]);
    expect_instantiation(
        global_import.clone(),
        HostRegistry::new(),
        |error| {
            matches!(
                error,
                RuntimeError::UnresolvedGlobalImport { module, name }
                    if module == "env" && name == "item"
            )
        },
        "unresolved global import",
    );

    let mut wrong_global_type = HostRegistry::new();
    wrong_global_type
        .register_global("env", "item", GlobalHandle::immutable(Value::I64(0)))
        .unwrap();
    expect_instantiation(
        global_import.clone(),
        wrong_global_type,
        |error| {
            matches!(
                error,
                RuntimeError::HostGlobalTypeMismatch {
                    expected: ValueType::I32,
                    actual: ValueType::I64,
                    ..
                }
            )
        },
        "host global type mismatch",
    );

    let mut wrong_global_mutability = HostRegistry::new();
    wrong_global_mutability
        .register_global("env", "item", GlobalHandle::mutable(Value::I32(0)))
        .unwrap();
    expect_instantiation(
        global_import,
        wrong_global_mutability,
        |error| {
            matches!(
                error,
                RuntimeError::HostGlobalMutabilityMismatch {
                    expected: false,
                    actual: true,
                    ..
                }
            )
        },
        "host global mutability mismatch",
    );

    let table_type = TableType {
        limits: Limits {
            min: 2,
            max: Some(4),
        },
    };
    let table_import = import_only(ImportDesc::Table(table_type), vec![]);
    expect_instantiation(
        table_import.clone(),
        HostRegistry::new(),
        |error| {
            matches!(
                error,
                RuntimeError::UnresolvedTableImport { module, name }
                    if module == "env" && name == "item"
            )
        },
        "unresolved table import",
    );

    let mut wrong_table_limits = HostRegistry::new();
    wrong_table_limits
        .register_table("env", "item", TableHandle::new(1, Some(4)).unwrap())
        .unwrap();
    expect_instantiation(
        table_import,
        wrong_table_limits,
        |error| matches!(error, RuntimeError::HostTableLimitsMismatch { .. }),
        "host table limits mismatch",
    );

    let memory_type = MemoryType {
        limits: Limits {
            min: 2,
            max: Some(4),
        },
    };
    let memory_import = import_only(ImportDesc::Memory(memory_type), vec![]);
    expect_instantiation(
        memory_import.clone(),
        HostRegistry::new(),
        |error| {
            matches!(
                error,
                RuntimeError::UnresolvedMemoryImport { module, name }
                    if module == "env" && name == "item"
            )
        },
        "unresolved memory import",
    );

    let mut wrong_memory_limits = HostRegistry::new();
    wrong_memory_limits
        .register_memory("env", "item", MemoryHandle::new(1, Some(4)).unwrap())
        .unwrap();
    expect_instantiation(
        memory_import,
        wrong_memory_limits,
        |error| matches!(error, RuntimeError::HostMemoryLimitsMismatch { .. }),
        "host memory limits mismatch",
    );
}

#[test]
fn active_segment_bounds_fail_only_during_instantiation() {
    let mut data_oob = parsed(function_module(&[], &[], &[], Some(1)));
    data_oob.data.push(DataSegment {
        mode: DataMode::Active {
            memory_index: 0,
            offset: 65_536,
        },
        bytes: vec![0xaa],
    });
    expect_instantiation(
        data_oob,
        HostRegistry::new(),
        |error| {
            matches!(
                error,
                RuntimeError::DataSegmentOutOfBounds {
                    segment: 0,
                    offset: 65_536,
                    length: 1
                }
            )
        },
        "active data segment runtime bounds",
    );

    let mut element_oob = parsed(function_module(&[], &[], &[], None));
    element_oob.tables.push(TableType {
        limits: Limits {
            min: 1,
            max: Some(1),
        },
    });
    element_oob.elements.push(ElementSegment {
        mode: ElementMode::Active {
            table_index: 0,
            offset: 1,
        },
        function_indices: vec![0],
    });
    expect_instantiation(
        element_oob,
        HostRegistry::new(),
        |error| {
            matches!(
                error,
                RuntimeError::ElementSegmentOutOfBounds {
                    segment: 0,
                    offset: 1,
                    length: 1
                }
            )
        },
        "active element segment runtime bounds",
    );
}

#[test]
fn differential_trap_classes_remain_execution_only() {
    let division = parsed(function_module(
        &[I32, I32],
        &[I32],
        &[0x20, 0x00, 0x20, 0x01, 0x6d],
        None,
    ));
    validate(&division).expect("signed division fixture must validate");
    let mut division_instance =
        Instance::new(division).expect("signed division fixture instantiates");
    assert!(matches!(
        division_instance.invoke_export("run", &[Value::I32(7), Value::I32(0)]),
        Err(RuntimeError::IntegerDivisionByZero)
    ));
    assert!(matches!(
        division_instance.invoke_export("run", &[Value::I32(i32::MIN), Value::I32(-1)]),
        Err(RuntimeError::IntegerOverflow)
    ));

    let load_oob = parsed(function_module(
        &[I32],
        &[I32],
        &[0x20, 0x00, 0x28, 0x02, 0x00],
        Some(1),
    ));
    validate(&load_oob).expect("load OOB fixture must validate");
    let mut load_instance = Instance::new(load_oob).expect("load OOB fixture instantiates");
    assert!(matches!(
        load_instance.invoke_export("run", &[Value::I32(65_535)]),
        Err(RuntimeError::MemoryOutOfBounds { width: 4, .. })
    ));

    let mut indirect_mismatch = parsed(superficial_indirect_module());
    indirect_mismatch.elements[0].function_indices[0] = 1;
    validate(&indirect_mismatch).expect("indirect mismatch fixture must validate");
    let mut indirect_instance =
        Instance::new(indirect_mismatch).expect("indirect mismatch fixture instantiates");
    assert!(matches!(
        indirect_instance.invoke_export("run", &[Value::I32(41), Value::I32(0)]),
        Err(RuntimeError::IndirectCallTypeMismatch {
            expected_type: 0,
            function_index: 1
        })
    ));
}

fn superficial_indirect_module() -> Vec<u8> {
    let mut bytes = header();
    push_section(
        &mut bytes,
        1,
        &[
            0x02, 0x60, 0x01, I32, 0x01, I32, 0x60, 0x02, I32, I32, 0x01, I32,
        ],
    );
    push_section(&mut bytes, 3, &[0x02, 0x00, 0x01]);
    push_section(&mut bytes, 4, &[0x01, 0x70, 0x00, 0x01]);
    push_section(&mut bytes, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x01]);
    push_section(&mut bytes, 9, &[0x01, 0x00, 0x41, 0x00, 0x0b, 0x01, 0x00]);

    let target = [0x00, 0x20, 0x00, 0x41, 0x01, 0x6a, 0x0b];
    let caller = [0x00, 0x20, 0x00, 0x20, 0x01, 0x11, 0x00, 0x00, 0x0b];
    let mut code = vec![0x02, target.len() as u8];
    code.extend(target);
    code.push(caller.len() as u8);
    code.extend(caller);
    push_section(&mut bytes, 10, &code);
    bytes
}
