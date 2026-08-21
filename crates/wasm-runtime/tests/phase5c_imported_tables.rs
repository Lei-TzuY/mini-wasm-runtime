use wasm_parser::parse_module;
use wasm_runtime::{
    HostRegistry, HostRegistryError, Instance, RuntimeError, TableHandle, TableHandleError, Value,
};

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

fn push_name(bytes: &mut Vec<u8>, name: &str) {
    push_u32(bytes, name.len() as u32);
    bytes.extend_from_slice(name.as_bytes());
}

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn imported_table_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(
        &mut module,
        1,
        &[
            0x02, // two function types
            0x60, 0x00, 0x01, 0x7f, // type 0: [] -> i32
            0x60, 0x01, 0x7f, 0x01, 0x7f, // type 1: [i32] -> i32
        ],
    );

    let mut imports = vec![0x01];
    push_name(&mut imports, "env");
    push_name(&mut imports, "tab");
    imports.extend([0x01, 0x70, 0x01, 0x02, 0x04]); // table funcref min=2 max=4
    push_section(&mut module, 2, &imports);

    push_section(&mut module, 3, &[0x02, 0x00, 0x01]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x01]);
    push_section(&mut module, 9, &[0x01, 0x00, 0x41, 0x00, 0x0b, 0x01, 0x00]);

    let target_body = [0x00, 0x41, 0x2a, 0x0b];
    let caller_body = [0x00, 0x20, 0x00, 0x11, 0x00, 0x00, 0x0b];
    let mut code = vec![0x02];
    push_u32(&mut code, target_body.len() as u32);
    code.extend(target_body);
    push_u32(&mut code, caller_body.len() as u32);
    code.extend(caller_body);
    push_section(&mut module, 10, &code);
    module
}

fn instantiate(table: &TableHandle) -> Instance {
    let module = parse_module(&imported_table_module()).expect("parse imported-table fixture");
    let mut hosts = HostRegistry::new();
    hosts
        .register_table("env", "tab", table.clone())
        .expect("register table");
    Instance::with_hosts(module, hosts).expect("instantiate imported-table fixture")
}

#[test]
fn active_element_initializes_shared_imported_table_and_call_indirect_executes() {
    let table = TableHandle::new(2, Some(4)).unwrap();
    let mut vm = instantiate(&table);
    assert!(table.get(0).unwrap().is_some());
    assert_eq!(
        vm.invoke_export("run", &[Value::I32(0)]).unwrap(),
        Some(Value::I32(42))
    );
}

#[test]
fn host_table_mutation_is_immediately_visible_to_call_indirect() {
    let table = TableHandle::new(2, Some(4)).unwrap();
    let mut vm = instantiate(&table);
    let target = table.get(0).unwrap().expect("element initialized slot 0");

    table.set(0, None).unwrap();
    assert!(matches!(
        vm.invoke_export("run", &[Value::I32(0)]),
        Err(RuntimeError::UninitializedTableElement(0))
    ));

    table.set(1, Some(target)).unwrap();
    assert_eq!(
        vm.invoke_export("run", &[Value::I32(1)]).unwrap(),
        Some(Value::I32(42))
    );
}

#[test]
fn imported_table_limits_follow_wasm_subtyping_rules() {
    for table in [
        TableHandle::new(1, Some(4)).unwrap(),
        TableHandle::new(2, None).unwrap(),
        TableHandle::new(2, Some(5)).unwrap(),
    ] {
        let module = parse_module(&imported_table_module()).unwrap();
        let mut hosts = HostRegistry::new();
        hosts.register_table("env", "tab", table).unwrap();
        assert!(matches!(
            Instance::with_hosts(module, hosts),
            Err(RuntimeError::HostTableLimitsMismatch { .. })
        ));
    }

    let wider_min_tighter_max = TableHandle::new(3, Some(3)).unwrap();
    let mut vm = instantiate(&wider_min_tighter_max);
    assert_eq!(
        vm.invoke_export("run", &[Value::I32(0)]).unwrap(),
        Some(Value::I32(42))
    );
}

#[test]
fn duplicate_table_registration_is_rejected() {
    let table = TableHandle::new(2, Some(4)).unwrap();
    let mut hosts = HostRegistry::new();
    hosts.register_table("env", "tab", table.clone()).unwrap();
    assert_eq!(
        hosts.register_table("env", "tab", table),
        Err(HostRegistryError::DuplicateTable {
            module: "env".into(),
            name: "tab".into(),
        })
    );
}

#[test]
fn one_table_handle_cannot_back_two_live_instances_yet() {
    let table = TableHandle::new(2, Some(4)).unwrap();
    let _first = instantiate(&table);

    let module = parse_module(&imported_table_module()).unwrap();
    let mut hosts = HostRegistry::new();
    hosts.register_table("env", "tab", table).unwrap();
    assert!(matches!(
        Instance::with_hosts(module, hosts),
        Err(RuntimeError::HostTableAlreadyBound { .. })
    ));
}

#[test]
fn stale_function_ref_never_aliases_same_numeric_index_in_new_instance() {
    let table = TableHandle::new(2, Some(4)).unwrap();
    let stale = {
        let _first = instantiate(&table);
        table.get(0).unwrap().expect("first instance writes slot 0")
    };

    let mut second = instantiate(&table);
    table.set(1, Some(stale)).unwrap();
    assert!(matches!(
        second.invoke_export("run", &[Value::I32(1)]),
        Err(RuntimeError::ForeignTableFunctionReference { element_index: 1 })
    ));
}

#[test]
fn table_handle_rejects_invalid_limits_and_oob_host_access() {
    assert_eq!(
        TableHandle::new(3, Some(2)).unwrap_err(),
        TableHandleError::InvalidLimits {
            minimum: 3,
            maximum: 2,
        }
    );
    let table = TableHandle::new(1, None).unwrap();
    assert!(matches!(
        table.get(1),
        Err(TableHandleError::OutOfBounds {
            index: 1,
            length: 1
        })
    ));
}
