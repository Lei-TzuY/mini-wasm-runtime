use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance as MiniInstance, RuntimeError, TableHandle, Value};
use wasmtime::{
    Engine, Extern, Instance as ReferenceInstance, Module as ReferenceModule, Ref, RefType, Store,
    Table, TableType, Trap as ReferenceTrap,
};

fn imported_table_wat() -> &'static str {
    r#"(module
        (type $unary (func (param i32) (result i32)))
        (import "env" "tab" (table 2 4 funcref))
        (func $add11 (type $unary)
            local.get 0
            i32.const 11
            i32.add)
        (func $xor (type $unary)
            local.get 0
            i32.const 1437226410
            i32.xor)
        (elem (i32.const 0) $add11 $xor)
        (func (export "run") (param i32 i32) (result i32)
            local.get 0
            local.get 1
            call_indirect (type $unary)))"#
}

fn expected(value: i32, slot: i32) -> i32 {
    match slot {
        0 => value.wrapping_add(11),
        1 => value ^ 1_437_226_410,
        other => panic!("unexpected fixture slot {other}"),
    }
}

fn make_mini(bytes: &[u8], table: TableHandle) -> MiniInstance {
    let mut hosts = HostRegistry::new();
    hosts
        .register_table("env", "tab", table)
        .expect("register mini imported table");
    MiniInstance::with_hosts(
        parse_module(bytes).expect("parse imported-table fixture"),
        hosts,
    )
    .expect("instantiate mini imported-table fixture")
}

fn make_reference(
    engine: &Engine,
    bytes: &[u8],
    minimum: u32,
    maximum: Option<u32>,
) -> (Store<()>, Table, ReferenceInstance) {
    let module = ReferenceModule::new(engine, bytes).expect("compile imported-table fixture");
    let mut store = Store::new(engine, ());
    let table = Table::new(
        &mut store,
        TableType::new(RefType::FUNCREF, minimum, maximum),
        Ref::Func(None),
    )
    .expect("create Wasmtime imported table");
    let instance = ReferenceInstance::new(&mut store, &module, &[Extern::Table(table)])
        .expect("instantiate Wasmtime imported-table fixture");
    (store, table, instance)
}

#[test]
fn imported_table_dispatch_and_host_mutation_match_wasmtime() {
    const SEED: u64 = 0x510e_527f_ade6_82d1;
    let bytes = wat::parse_str(imported_table_wat()).expect("compile imported-table WAT");
    let mini_table = TableHandle::new(2, Some(4)).expect("create mini imported table");
    let mut mini = make_mini(&bytes, mini_table.clone());

    let engine = Engine::default();
    let (mut store, reference_table, reference) = make_reference(&engine, &bytes, 2, Some(4));
    let reference_run = reference
        .get_typed_func::<(i32, i32), i32>(&mut store, "run")
        .expect("Wasmtime run export must be [i32, i32] -> [i32]");

    let mut state = SEED;
    for case in 0..64 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let value = state as u32 as i32;
        let slot = case & 1;
        let expected = expected(value, slot);

        let mini_value = mini
            .invoke_export_values("run", &[Value::I32(value), Value::I32(slot)])
            .unwrap_or_else(|error| {
                panic!(
                    "mini imported-table call failed at seed={SEED:#018x} case={case}: {error:?}"
                )
            });
        let mini_value = match mini_value.as_slice() {
            [Value::I32(value)] => *value,
            other => panic!("unexpected mini imported-table result shape: {other:?}"),
        };
        let reference_value = reference_run
            .call(&mut store, (value, slot))
            .unwrap_or_else(|error| {
                panic!("Wasmtime imported-table call failed at seed={SEED:#018x} case={case}: {error:?}")
            });

        assert_eq!(mini_value, expected, "mini mismatch at case={case}");
        assert_eq!(
            reference_value, expected,
            "Wasmtime mismatch at case={case}"
        );
        assert_eq!(
            mini_value, reference_value,
            "differential mismatch at case={case}"
        );
    }

    let mini_slot_zero = mini_table
        .get(0)
        .expect("read mini imported table slot 0")
        .expect("element segment must initialize mini slot 0");
    mini_table
        .set(1, Some(mini_slot_zero))
        .expect("move mini slot 0 target to slot 1");
    let reference_slot_zero = reference_table
        .get(&mut store, 0)
        .expect("element segment must initialize Wasmtime slot 0");
    reference_table
        .set(&mut store, 1, reference_slot_zero)
        .expect("move Wasmtime slot 0 target to slot 1");

    let value = -123_456_789_i32;
    let moved_expected = value.wrapping_add(11);
    let mini_moved = mini
        .invoke_export_values("run", &[Value::I32(value), Value::I32(1)])
        .expect("mini moved target must remain callable");
    assert_eq!(mini_moved, vec![Value::I32(moved_expected)]);
    assert_eq!(
        reference_run.call(&mut store, (value, 1)).unwrap(),
        moved_expected
    );

    mini_table.set(0, None).expect("clear mini slot 0");
    reference_table
        .set(&mut store, 0, Ref::Func(None))
        .expect("clear Wasmtime slot 0");
    assert!(matches!(
        mini.invoke_export_values("run", &[Value::I32(7), Value::I32(0)]),
        Err(RuntimeError::UninitializedTableElement(0))
    ));
    let reference_error = reference_run
        .call(&mut store, (7, 0))
        .expect_err("Wasmtime null table slot must trap");
    assert_eq!(
        reference_error.downcast_ref::<ReferenceTrap>(),
        Some(&ReferenceTrap::IndirectCallToNull)
    );
}

#[test]
fn imported_table_limit_matching_agrees_with_wasmtime() {
    let bytes = wat::parse_str(imported_table_wat()).expect("compile imported-table WAT");
    let engine = Engine::default();
    let reference_module =
        ReferenceModule::new(&engine, &bytes).expect("compile Wasmtime imported-table module");

    for (minimum, maximum) in [(1_u32, Some(4_u32)), (2, None), (2, Some(5))] {
        let mini_table = TableHandle::new(minimum, maximum).expect("create mini mismatch table");
        let mut hosts = HostRegistry::new();
        hosts.register_table("env", "tab", mini_table).unwrap();
        let mini_rejected = MiniInstance::with_hosts(parse_module(&bytes).unwrap(), hosts).is_err();

        let mut store = Store::new(&engine, ());
        let reference_table = Table::new(
            &mut store,
            TableType::new(RefType::FUNCREF, minimum, maximum),
            Ref::Func(None),
        )
        .unwrap();
        let reference_rejected = ReferenceInstance::new(
            &mut store,
            &reference_module,
            &[Extern::Table(reference_table)],
        )
        .is_err();

        assert!(
            mini_rejected,
            "mini accepted mismatched limits {minimum} {maximum:?}"
        );
        assert!(
            reference_rejected,
            "Wasmtime accepted mismatched limits {minimum} {maximum:?}"
        );
    }

    let mini_table = TableHandle::new(3, Some(3)).expect("create compatible mini table");
    let mut mini = make_mini(&bytes, mini_table);
    let (mut store, _, reference) = make_reference(&engine, &bytes, 3, Some(3));
    let reference_run = reference
        .get_typed_func::<(i32, i32), i32>(&mut store, "run")
        .unwrap();
    assert_eq!(
        mini.invoke_export_values("run", &[Value::I32(31), Value::I32(0)])
            .unwrap(),
        vec![Value::I32(42)]
    );
    assert_eq!(reference_run.call(&mut store, (31, 0)).unwrap(), 42);
}
