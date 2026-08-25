use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, Value};

const I32: u8 = 0x7f;
const I64: u8 = 0x7e;
const F32: u8 = 0x7d;
const F64: u8 = 0x7c;
const UPSTREAM_SPEC_COMMIT: &str = "fc209c5ed8afc4dfeb9252024d217da3376c7a6f";

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

fn push_signed(bytes: &mut Vec<u8>, mut value: i64) {
    loop {
        let byte = (value as u8) & 0x7f;
        let sign_bit_set = byte & 0x40 != 0;
        value >>= 7;
        let done = (value == 0 && !sign_bit_set) || (value == -1 && sign_bit_set);
        bytes.push(if done { byte } else { byte | 0x80 });
        if done {
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

fn push_export(payload: &mut Vec<u8>, name: &str, function_index: u8) {
    push_name(payload, name);
    payload.extend([0x00, function_index]);
}

fn push_body(payload: &mut Vec<u8>, instructions: &[u8]) {
    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    push_u32(payload, body.len() as u32);
    payload.extend_from_slice(&body);
}

fn push_global_i32(payload: &mut Vec<u8>, mutable: bool, value: i32) {
    payload.extend([I32, u8::from(mutable), 0x41]);
    push_signed(payload, i64::from(value));
    payload.push(0x0b);
}

fn push_global_i64(payload: &mut Vec<u8>, mutable: bool, value: i64) {
    payload.extend([I64, u8::from(mutable), 0x42]);
    push_signed(payload, value);
    payload.push(0x0b);
}

fn push_global_f32(payload: &mut Vec<u8>, mutable: bool, value: f32) {
    payload.extend([F32, u8::from(mutable), 0x43]);
    payload.extend_from_slice(&value.to_bits().to_le_bytes());
    payload.push(0x0b);
}

fn push_global_f64(payload: &mut Vec<u8>, mutable: bool, value: f64) {
    payload.extend([F64, u8::from(mutable), 0x44]);
    payload.extend_from_slice(&value.to_bits().to_le_bytes());
    payload.push(0x0b);
}

fn global_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(
        &mut module,
        1,
        &[
            0x08, 0x60, 0x00, 0x01, I32, 0x60, 0x00, 0x01, I64, 0x60, 0x00, 0x01, F32, 0x60,
            0x00, 0x01, F64, 0x60, 0x01, I32, 0x00, 0x60, 0x01, I64, 0x00, 0x60, 0x01, F32,
            0x00, 0x60, 0x01, F64, 0x00,
        ],
    );

    let mut imports = vec![0x02];
    push_name(&mut imports, "spectest");
    push_name(&mut imports, "global_i32");
    imports.extend([0x03, I32, 0x00]);
    push_name(&mut imports, "spectest");
    push_name(&mut imports, "global_i64");
    imports.extend([0x03, I64, 0x00]);
    push_section(&mut module, 2, &imports);

    push_section(
        &mut module,
        3,
        &[0x0c, 0x00, 0x01, 0x00, 0x01, 0x02, 0x03, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07],
    );

    let mut globals = vec![0x08];
    push_global_i32(&mut globals, false, -2);
    push_global_f32(&mut globals, false, -3.0);
    push_global_f64(&mut globals, false, -4.0);
    push_global_i64(&mut globals, false, -5);
    push_global_i32(&mut globals, true, -12);
    push_global_f32(&mut globals, true, -13.0);
    push_global_f64(&mut globals, true, -14.0);
    push_global_i64(&mut globals, true, -15);
    push_section(&mut module, 6, &globals);

    let mut exports = vec![0x0c];
    for (name, index) in [
        ("get-a", 0),
        ("get-b", 1),
        ("get-x", 2),
        ("get-y", 3),
        ("get-3", 4),
        ("get-4", 5),
        ("get-7", 6),
        ("get-8", 7),
        ("set-x", 8),
        ("set-y", 9),
        ("set-7", 10),
        ("set-8", 11),
    ] {
        push_export(&mut exports, name, index);
    }
    push_section(&mut module, 7, &exports);

    let mut code = vec![0x0c];
    push_body(&mut code, &[0x23, 0x02]);
    push_body(&mut code, &[0x23, 0x05]);
    push_body(&mut code, &[0x23, 0x06]);
    push_body(&mut code, &[0x23, 0x09]);
    push_body(&mut code, &[0x23, 0x03]);
    push_body(&mut code, &[0x23, 0x04]);
    push_body(&mut code, &[0x23, 0x07]);
    push_body(&mut code, &[0x23, 0x08]);
    push_body(&mut code, &[0x20, 0x00, 0x24, 0x06]);
    push_body(&mut code, &[0x20, 0x00, 0x24, 0x09]);
    push_body(&mut code, &[0x20, 0x00, 0x24, 0x07]);
    push_body(&mut code, &[0x20, 0x00, 0x24, 0x08]);
    push_section(&mut module, 10, &code);

    module
}

fn instance() -> Instance {
    let module = parse_module(&global_module()).expect("global spec vector must parse");
    let mut hosts = HostRegistry::new();
    hosts
        .register_immutable_global("spectest", "global_i32", Value::I32(666))
        .expect("register spectest global_i32");
    hosts
        .register_immutable_global("spectest", "global_i64", Value::I64(666))
        .expect("register spectest global_i64");
    Instance::with_hosts(module, hosts).expect("global spec vector must validate and instantiate")
}

#[test]
fn upstream_numeric_global_initializers_match_supported_spec_subset() {
    // WebAssembly/spec test/core/global.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let mut vm = instance();
    for (name, expected) in [
        ("get-a", Value::I32(-2)),
        ("get-b", Value::I64(-5)),
        ("get-x", Value::I32(-12)),
        ("get-y", Value::I64(-15)),
        ("get-3", Value::F32(-3.0)),
        ("get-4", Value::F64(-4.0)),
        ("get-7", Value::F32(-13.0)),
        ("get-8", Value::F64(-14.0)),
    ] {
        assert_eq!(
            vm.invoke_export(name, &[])
                .expect("supported global getter must execute"),
            Some(expected),
            "unexpected result for {name}"
        );
    }
}

#[test]
fn upstream_mutable_numeric_globals_round_trip_all_supported_types() {
    let mut vm = instance();

    assert_eq!(vm.invoke_export("set-x", &[Value::I32(6)]).unwrap(), None);
    assert_eq!(vm.invoke_export("set-y", &[Value::I64(7)]).unwrap(), None);
    assert_eq!(vm.invoke_export("set-7", &[Value::F32(8.0)]).unwrap(), None);
    assert_eq!(vm.invoke_export("set-8", &[Value::F64(9.0)]).unwrap(), None);

    assert_eq!(vm.invoke_export("get-x", &[]).unwrap(), Some(Value::I32(6)));
    assert_eq!(vm.invoke_export("get-y", &[]).unwrap(), Some(Value::I64(7)));
    assert_eq!(vm.invoke_export("get-7", &[]).unwrap(), Some(Value::F32(8.0)));
    assert_eq!(vm.invoke_export("get-8", &[]).unwrap(), Some(Value::F64(9.0)));
}
