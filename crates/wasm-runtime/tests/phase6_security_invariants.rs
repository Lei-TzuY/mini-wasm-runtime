use std::{cell::Cell, rc::Rc};

use wasm_parser::{
    Export, ExportKind, FuncType, FunctionBody, Import, ImportDesc, Limits, MemoryType, Module,
    ValueType,
};
use wasm_runtime::{
    HostCapabilities, HostError, HostRegistry, Instance, RuntimeError, RuntimeLimits, Value,
    WASM_PAGE_SIZE,
};

fn host_write_module() -> Module {
    Module {
        types: vec![
            FuncType {
                params: vec![],
                results: vec![],
            },
            FuncType {
                params: vec![],
                results: vec![ValueType::I32],
            },
        ],
        imports: vec![Import {
            module: "env".into(),
            name: "write".into(),
            desc: ImportDesc::Function(0),
        }],
        function_type_indices: vec![1],
        memories: vec![MemoryType {
            limits: Limits {
                min: 1,
                max: Some(1),
            },
        }],
        exports: vec![
            Export {
                name: "write".into(),
                kind: ExportKind::Function,
                index: 0,
            },
            Export {
                name: "read".into(),
                kind: ExportKind::Function,
                index: 1,
            },
        ],
        code: vec![FunctionBody {
            locals: vec![],
            code: vec![0x41, 0x00, 0x28, 0x02, 0x00, 0x0b],
        }],
        ..Module::default()
    }
}

fn read_zero_word(instance: &mut Instance) -> i32 {
    match instance.invoke_export("read", &[]).unwrap() {
        Some(Value::I32(value)) => value,
        other => panic!("read export returned unexpected result: {other:?}"),
    }
}

#[test]
fn host_call_budget_is_enforced_before_callback_side_effects() {
    let called = Rc::new(Cell::new(false));
    let called_by_host = called.clone();
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "write",
            vec![],
            vec![],
            HostCapabilities::MEMORY_READ_WRITE,
            move |ctx, _args| {
                called_by_host.set(true);
                ctx.write_memory(0, &[0x78, 0x56, 0x34, 0x12])?;
                Ok(None)
            },
        )
        .unwrap();

    let limits = RuntimeLimits {
        max_host_calls: Some(0),
        ..RuntimeLimits::default()
    };
    let mut instance = Instance::with_config(host_write_module(), hosts, limits).unwrap();

    assert!(matches!(
        instance.invoke_export("write", &[]),
        Err(RuntimeError::HostCallLimitExceeded { limit: 0 })
    ));
    assert!(!called.get(), "host-call budget must reject before callback entry");
    assert_eq!(
        read_zero_word(&mut instance),
        0,
        "budget rejection must not leave a host memory side effect"
    );
}

#[test]
fn denied_host_memory_write_is_side_effect_free() {
    let called = Rc::new(Cell::new(false));
    let called_by_host = called.clone();
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "write",
            vec![],
            vec![],
            HostCapabilities::NONE,
            move |ctx, _args| {
                called_by_host.set(true);
                ctx.write_memory(0, &[0x78, 0x56, 0x34, 0x12])?;
                Ok(None)
            },
        )
        .unwrap();

    let mut instance = Instance::with_hosts(host_write_module(), hosts).unwrap();
    assert!(matches!(
        instance.invoke_export("write", &[]),
        Err(RuntimeError::HostCallFailed {
            error: HostError::CapabilityDenied("memory.write"),
            ..
        })
    ));
    assert!(called.get(), "the callback must be entered before capability use");
    assert_eq!(
        read_zero_word(&mut instance),
        0,
        "capability denial must happen before bytes are copied"
    );
}

#[test]
fn out_of_bounds_host_memory_write_is_all_or_nothing() {
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "write",
            vec![],
            vec![],
            HostCapabilities::MEMORY_READ_WRITE,
            |ctx, _args| {
                ctx.write_memory((WASM_PAGE_SIZE - 2) as u32, &[1, 2, 3, 4])?;
                Ok(None)
            },
        )
        .unwrap();

    let mut instance = Instance::with_hosts(host_write_module(), hosts).unwrap();
    assert!(matches!(
        instance.invoke_export("write", &[]),
        Err(RuntimeError::HostCallFailed {
            error: HostError::MemoryOutOfBounds { address, width: 4 },
            ..
        }) if address == (WASM_PAGE_SIZE - 2) as u64
    ));

    let memory = instance.memory().expect("test module defines owned memory");
    assert!(
        memory.bytes()[WASM_PAGE_SIZE - 4..].iter().all(|&byte| byte == 0),
        "bounds failure must precede any partial memory copy"
    );
}

#[test]
fn host_result_validation_is_not_a_transaction_boundary() {
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "write",
            vec![],
            vec![],
            HostCapabilities::MEMORY_READ_WRITE,
            |ctx, _args| {
                ctx.write_memory(0, &[0x78, 0x56, 0x34, 0x12])?;
                Ok(Some(Value::I32(7)))
            },
        )
        .unwrap();

    let mut instance = Instance::with_hosts(host_write_module(), hosts).unwrap();
    assert!(matches!(
        instance.invoke_export("write", &[]),
        Err(RuntimeError::HostResultArityMismatch {
            expected: 0,
            actual: 1,
            ..
        })
    ));
    assert_eq!(
        read_zero_word(&mut instance),
        0x1234_5678,
        "authorized callback effects are not rolled back by post-callback result validation"
    );
}

fn grow_module() -> Module {
    Module {
        types: vec![FuncType {
            params: vec![],
            results: vec![ValueType::I32],
        }],
        function_type_indices: vec![0],
        memories: vec![MemoryType {
            limits: Limits {
                min: 1,
                max: Some(2),
            },
        }],
        exports: vec![Export {
            name: "grow".into(),
            kind: ExportKind::Function,
            index: 0,
        }],
        code: vec![FunctionBody {
            locals: vec![],
            code: vec![0x41, 0x01, 0x40, 0x00, 0x0b],
        }],
        ..Module::default()
    }
}

#[test]
fn fuel_is_consumed_before_side_effecting_memory_grow() {
    let limits = RuntimeLimits {
        fuel: Some(1),
        ..RuntimeLimits::default()
    };
    let mut instance = Instance::with_config(grow_module(), HostRegistry::new(), limits).unwrap();
    assert_eq!(instance.memory().unwrap().size_pages(), 1);

    assert!(matches!(
        instance.invoke_export("grow", &[]),
        Err(RuntimeError::FuelExhausted)
    ));
    assert_eq!(
        instance.memory().unwrap().size_pages(),
        1,
        "fuel exhaustion must occur before memory.grow mutates the instance"
    );
}
