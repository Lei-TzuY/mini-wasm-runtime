use wasm_parser::parse_module;
use wasm_runtime::{
    GlobalHandle, GlobalHandleError, HostRegistry, HostRegistryError, Instance, RuntimeError, Value,
};

const I32: u8 = 0x7f;
const I64: u8 = 0x7e;
const F32: u8 = 0x7d;
const F64: u8 = 0x7c;

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

fn module_header() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

fn imported_global_getter(value_type: u8, mutable: bool) -> Vec<u8> {
    let mut module = module_header();
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x01, value_type]);

    let mut imports = vec![0x01];
    push_name(&mut imports, "env");
    push_name(&mut imports, "g");
    imports.extend([0x03, value_type, u8::from(mutable)]);
    push_section(&mut module, 2, &imports);

    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(
        &mut module,
        7,
        &[
            0x02, 0x03, b'r', b'u', b'n', 0x00, 0x00, 0x01, b'g', 0x03, 0x00,
        ],
    );
    push_section(&mut module, 10, &[0x01, 0x04, 0x00, 0x23, 0x00, 0x0b]);
    module
}

fn imported_and_defined_global_module() -> Vec<u8> {
    let mut module = module_header();
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x01, I32]);

    let mut imports = vec![0x01];
    push_name(&mut imports, "env");
    push_name(&mut imports, "base");
    imports.extend([0x03, I32, 0x00]);
    push_section(&mut module, 2, &imports);

    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 6, &[0x01, I32, 0x01, 0x41, 0x00, 0x0b]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);

    let body = [
        0x00, // no locals
        0x41, 0x09, // i32.const 9
        0x24, 0x01, // global.set 1 (defined mutable global)
        0x23, 0x00, // global.get 0 (imported immutable global)
        0x23, 0x01, // global.get 1
        0x6a, // i32.add
        0x0b,
    ];
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend(body);
    push_section(&mut module, 10, &code);
    module
}

fn instantiate_with_global(bytes: &[u8], name: &str, value: Value) -> Instance {
    let module = parse_module(bytes).expect("parse imported-global fixture");
    let mut hosts = HostRegistry::new();
    hosts
        .register_immutable_global("env", name, value)
        .expect("register immutable global");
    Instance::with_hosts(module, hosts).expect("instantiate imported-global fixture")
}

#[test]
fn immutable_imported_globals_execute_for_all_numeric_types() {
    let cases = [
        (I32, Value::I32(-7)),
        (I64, Value::I64(0x1_0000_0001)),
        (F32, Value::F32(1.25)),
        (F64, Value::F64(-9.5)),
    ];

    for (value_type, value) in cases {
        let module = imported_global_getter(value_type, false);
        let mut vm = instantiate_with_global(&module, "g", value);
        assert_eq!(vm.global(0), Some(value));
        assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(value));
    }
}

#[test]
fn imported_global_precedes_defined_global_in_runtime_index_space() {
    let module = imported_and_defined_global_module();
    let mut vm = instantiate_with_global(&module, "base", Value::I32(33));
    assert_eq!(vm.global(0), Some(Value::I32(33)));
    assert_eq!(vm.global(1), Some(Value::I32(0)));
    assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(42)));
    assert_eq!(vm.global(1), Some(Value::I32(9)));
}

#[test]
fn missing_immutable_global_binding_is_distinct_from_unsupported_object_import() {
    let module = parse_module(&imported_global_getter(I32, false)).unwrap();
    assert!(matches!(
        Instance::new(module),
        Err(RuntimeError::UnresolvedGlobalImport { ref module, ref name })
            if module == "env" && name == "g"
    ));
}

#[test]
fn immutable_global_binding_requires_exact_numeric_type() {
    let module = parse_module(&imported_global_getter(I32, false)).unwrap();
    let mut hosts = HostRegistry::new();
    hosts
        .register_immutable_global("env", "g", Value::I64(7))
        .unwrap();
    assert!(matches!(
        Instance::with_hosts(module, hosts),
        Err(RuntimeError::HostGlobalTypeMismatch {
            expected: wasm_parser::ValueType::I32,
            actual: wasm_parser::ValueType::I64,
            ..
        })
    ));
}

fn mutable_global_accessors() -> Vec<u8> {
    let mut module = module_header();
    let types = [
        0x02, 0x60, 0x00, 0x01, I32, // [] -> i32
        0x60, 0x01, I32, 0x00, // [i32] -> []
    ];
    push_section(&mut module, 1, &types);

    let mut imports = vec![0x01];
    push_name(&mut imports, "env");
    push_name(&mut imports, "g");
    imports.extend([0x03, I32, 0x01]);
    push_section(&mut module, 2, &imports);

    push_section(&mut module, 3, &[0x02, 0x00, 0x01]);
    push_section(
        &mut module,
        7,
        &[
            0x02, 0x03, b'g', b'e', b't', 0x00, 0x00, 0x03, b's', b'e', b't', 0x00, 0x01,
        ],
    );

    let getter = [0x00, 0x23, 0x00, 0x0b];
    let setter = [0x00, 0x20, 0x00, 0x24, 0x00, 0x0b];
    let mut code = vec![0x02];
    push_u32(&mut code, getter.len() as u32);
    code.extend(getter);
    push_u32(&mut code, setter.len() as u32);
    code.extend(setter);
    push_section(&mut module, 10, &code);
    module
}

#[test]
fn mutable_global_import_preserves_bidirectional_aliasing() {
    let module = parse_module(&mutable_global_accessors()).unwrap();
    let global = GlobalHandle::mutable(Value::I32(7));
    let mut hosts = HostRegistry::new();
    hosts.register_global("env", "g", global.clone()).unwrap();
    let mut vm = Instance::with_hosts(module, hosts).unwrap();

    global.set(Value::I32(11)).unwrap();
    assert_eq!(vm.invoke_export("get", &[]).unwrap(), Some(Value::I32(11)));

    assert_eq!(vm.invoke_export("set", &[Value::I32(42)]).unwrap(), None);
    assert_eq!(global.get(), Value::I32(42));
    assert_eq!(vm.global(0), Some(Value::I32(42)));
}

#[test]
fn imported_global_mutability_must_match_exactly() {
    let mutable_module = parse_module(&imported_global_getter(I32, true)).unwrap();
    let mut immutable_hosts = HostRegistry::new();
    immutable_hosts
        .register_immutable_global("env", "g", Value::I32(7))
        .unwrap();
    assert!(matches!(
        Instance::with_hosts(mutable_module, immutable_hosts),
        Err(RuntimeError::HostGlobalMutabilityMismatch {
            expected: true,
            actual: false,
            ..
        })
    ));

    let immutable_module = parse_module(&imported_global_getter(I32, false)).unwrap();
    let mut mutable_hosts = HostRegistry::new();
    mutable_hosts
        .register_global("env", "g", GlobalHandle::mutable(Value::I32(7)))
        .unwrap();
    assert!(matches!(
        Instance::with_hosts(immutable_module, mutable_hosts),
        Err(RuntimeError::HostGlobalMutabilityMismatch {
            expected: false,
            actual: true,
            ..
        })
    ));
}

#[test]
fn global_handle_enforces_mutability_and_type_outside_the_vm() {
    let immutable = GlobalHandle::immutable(Value::I32(1));
    assert_eq!(
        immutable.set(Value::I32(2)),
        Err(GlobalHandleError::Immutable)
    );

    let mutable = GlobalHandle::mutable(Value::I32(1));
    assert_eq!(
        mutable.set(Value::I64(2)),
        Err(GlobalHandleError::TypeMismatch {
            expected: wasm_parser::ValueType::I32,
            actual: wasm_parser::ValueType::I64,
        })
    );
    assert_eq!(mutable.get(), Value::I32(1));
}

#[test]
fn duplicate_immutable_global_registration_is_rejected() {
    let mut hosts = HostRegistry::new();
    hosts
        .register_immutable_global("env", "g", Value::I32(1))
        .unwrap();
    assert_eq!(
        hosts.register_immutable_global("env", "g", Value::I32(2)),
        Err(HostRegistryError::DuplicateGlobal {
            module: "env".into(),
            name: "g".into(),
        })
    );
}
