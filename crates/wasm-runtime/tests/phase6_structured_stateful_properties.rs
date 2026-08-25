use std::collections::HashMap;

use wasm_parser::parse_module;
use wasm_runtime::{
    GlobalHandle, HostRegistry, Instance, MemoryHandle, RuntimeError, Value,
};

const I32: u8 = 0x7f;
const I64: u8 = 0x7e;
const SEED: u64 = 0x6a09_e667_f3bc_c909;

struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        assert_ne!(seed, 0, "xorshift seed must be non-zero");
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        let mut value = self.state;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.state = value;
        value
    }

    fn next_i32(&mut self) -> i32 {
        self.next_u64() as u32 as i32
    }
}

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

fn header() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

fn multi_value_if_module(
    then_i32: u8,
    then_i64: u8,
    else_i32: u8,
    else_i64: u8,
) -> Vec<u8> {
    debug_assert!(then_i32 < 64 && then_i64 < 64 && else_i32 < 64 && else_i64 < 64);

    let mut module = header();
    let types = [
        0x02,
        0x60,
        0x01,
        I32,
        0x02,
        I32,
        I64,
        0x60,
        0x00,
        0x02,
        I32,
        I64,
    ];
    push_section(&mut module, 1, &types);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(
        &mut module,
        7,
        &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00],
    );

    let body = [
        0x00,
        0x20,
        0x00,
        0x04,
        0x01,
        0x41,
        then_i32,
        0x42,
        then_i64,
        0x05,
        0x41,
        else_i32,
        0x42,
        else_i64,
        0x0b,
        0x0b,
    ];
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn indirect_dispatch_module(first_delta: u8, second_delta: u8) -> Vec<u8> {
    debug_assert!(first_delta < 64 && second_delta < 64);

    let mut module = header();
    let types = [
        0x02,
        0x60,
        0x01,
        I32,
        0x01,
        I32,
        0x60,
        0x02,
        I32,
        I32,
        0x01,
        I32,
    ];
    push_section(&mut module, 1, &types);
    push_section(&mut module, 3, &[0x03, 0x00, 0x00, 0x01]);
    push_section(&mut module, 4, &[0x01, 0x70, 0x00, 0x03]);
    push_section(
        &mut module,
        7,
        &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x02],
    );
    push_section(
        &mut module,
        9,
        &[0x01, 0x00, 0x41, 0x00, 0x0b, 0x02, 0x00, 0x01],
    );

    let first = [0x00, 0x20, 0x00, 0x41, first_delta, 0x6a, 0x0b];
    let second = [0x00, 0x20, 0x00, 0x41, second_delta, 0x6a, 0x0b];
    let dispatcher = [
        0x00, 0x20, 0x00, 0x20, 0x01, 0x11, 0x00, 0x00, 0x0b,
    ];

    let mut code = vec![0x03];
    for body in [&first[..], &second[..], &dispatcher[..]] {
        push_u32(&mut code, body.len() as u32);
        code.extend_from_slice(body);
    }
    push_section(&mut module, 10, &code);
    module
}

fn imported_global_accumulator_module() -> Vec<u8> {
    let mut module = header();
    push_section(
        &mut module,
        1,
        &[0x01, 0x60, 0x01, I32, 0x01, I32],
    );

    let mut imports = vec![0x01];
    push_name(&mut imports, "env");
    push_name(&mut imports, "g");
    imports.extend_from_slice(&[0x03, I32, 0x01]);
    push_section(&mut module, 2, &imports);

    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(
        &mut module,
        7,
        &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00],
    );

    let body = [
        0x00,
        0x23,
        0x00,
        0x20,
        0x00,
        0x6a,
        0x24,
        0x00,
        0x23,
        0x00,
        0x0b,
    ];
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn imported_memory_state_module() -> Vec<u8> {
    let mut module = header();
    let types = [
        0x02,
        0x60,
        0x01,
        I32,
        0x01,
        I32,
        0x60,
        0x02,
        I32,
        I32,
        0x00,
    ];
    push_section(&mut module, 1, &types);

    let mut imports = vec![0x01];
    push_name(&mut imports, "env");
    push_name(&mut imports, "mem");
    imports.extend_from_slice(&[0x02, 0x01, 0x01, 0x01]);
    push_section(&mut module, 2, &imports);

    push_section(&mut module, 3, &[0x02, 0x00, 0x01]);

    let mut exports = vec![0x02];
    for (name, index) in [("load", 0u32), ("store", 1u32)] {
        push_name(&mut exports, name);
        exports.push(0x00);
        push_u32(&mut exports, index);
    }
    push_section(&mut module, 7, &exports);

    let load = [0x00, 0x20, 0x00, 0x28, 0x02, 0x00, 0x0b];
    let store = [
        0x00, 0x20, 0x00, 0x20, 0x01, 0x36, 0x02, 0x00, 0x0b,
    ];
    let mut code = vec![0x02];
    for body in [&load[..], &store[..]] {
        push_u32(&mut code, body.len() as u32);
        code.extend_from_slice(body);
    }
    push_section(&mut module, 10, &code);
    module
}

#[test]
fn generated_multi_value_if_preserves_order_and_branch_selection() {
    let mut rng = XorShift64::new(SEED);

    for case in 0..128 {
        let then_i32 = (rng.next_u64() % 64) as u8;
        let then_i64 = (rng.next_u64() % 64) as u8;
        let else_i32 = (rng.next_u64() % 64) as u8;
        let else_i64 = (rng.next_u64() % 64) as u8;
        let condition = rng.next_i32();

        let module = parse_module(&multi_value_if_module(
            then_i32, then_i64, else_i32, else_i64,
        ))
        .expect("generated multi-value fixture must parse");
        let mut instance =
            Instance::new(module).expect("generated multi-value fixture must instantiate");
        let expected = if condition != 0 {
            vec![Value::I32(i32::from(then_i32)), Value::I64(i64::from(then_i64))]
        } else {
            vec![Value::I32(i32::from(else_i32)), Value::I64(i64::from(else_i64))]
        };

        assert_eq!(
            instance
                .invoke_export_values("run", &[Value::I32(condition)])
                .unwrap(),
            expected,
            "multi-value structured mismatch at seed={SEED:#018x} case={case} condition={condition}"
        );
    }
}

#[test]
fn generated_table_dispatch_distinguishes_targets_null_and_oob() {
    let mut rng = XorShift64::new(SEED ^ 0xbb67_ae85_84ca_a73b);

    for case in 0..128 {
        let first_delta = (rng.next_u64() % 32) as u8;
        let second_delta = (rng.next_u64() % 32) as u8;
        let value = rng.next_i32();
        let selector = (rng.next_u64() % 5) as i32;

        let module = parse_module(&indirect_dispatch_module(first_delta, second_delta))
            .expect("generated table fixture must parse");
        let mut instance =
            Instance::new(module).expect("generated table fixture must instantiate");
        let observed =
            instance.invoke_export("run", &[Value::I32(value), Value::I32(selector)]);

        match selector {
            0 => assert_eq!(
                observed.unwrap(),
                Some(Value::I32(value.wrapping_add(i32::from(first_delta)))),
                "first table target mismatch at seed={SEED:#018x} case={case}"
            ),
            1 => assert_eq!(
                observed.unwrap(),
                Some(Value::I32(value.wrapping_add(i32::from(second_delta)))),
                "second table target mismatch at seed={SEED:#018x} case={case}"
            ),
            2 => assert!(
                matches!(
                    observed,
                    Err(RuntimeError::UninitializedTableElement(2))
                ),
                "expected null-table trap at seed={SEED:#018x} case={case}, observed={observed:?}"
            ),
            _ => assert!(
                matches!(
                    observed,
                    Err(RuntimeError::TableElementOutOfBounds(index)) if index == selector as u32
                ),
                "expected table-OOB trap at seed={SEED:#018x} case={case}, selector={selector}, observed={observed:?}"
            ),
        }
    }
}

#[test]
fn generated_imported_global_sequence_matches_independent_state_model() {
    let module = parse_module(&imported_global_accumulator_module())
        .expect("generated imported-global fixture must parse");
    let global = GlobalHandle::mutable(Value::I32(0));
    let mut hosts = HostRegistry::new();
    hosts
        .register_global("env", "g", global.clone())
        .expect("register generated imported global");
    let mut instance =
        Instance::with_hosts(module, hosts).expect("generated imported-global fixture must instantiate");

    let mut rng = XorShift64::new(SEED ^ 0x3c6e_f372_fe94_f82b);
    let mut model = 0i32;

    for step in 0..256 {
        if rng.next_u64() % 4 == 0 {
            model = rng.next_i32();
            global
                .set(Value::I32(model))
                .expect("host override must preserve imported-global type");
            assert_eq!(
                instance.invoke_export("run", &[Value::I32(0)]).unwrap(),
                Some(Value::I32(model)),
                "guest did not observe host global override at seed={SEED:#018x} step={step}"
            );
        } else {
            let delta = rng.next_i32();
            model = model.wrapping_add(delta);
            assert_eq!(
                instance.invoke_export("run", &[Value::I32(delta)]).unwrap(),
                Some(Value::I32(model)),
                "guest global transition mismatch at seed={SEED:#018x} step={step} delta={delta}"
            );
        }

        assert_eq!(
            global.get(),
            Value::I32(model),
            "host alias diverged at seed={SEED:#018x} step={step}"
        );
        assert_eq!(
            instance.global(0),
            Some(Value::I32(model)),
            "instance global view diverged at seed={SEED:#018x} step={step}"
        );
    }
}

#[test]
fn generated_stateful_memory_sequence_preserves_host_guest_aliasing() {
    let module = parse_module(&imported_memory_state_module())
        .expect("generated imported-memory fixture must parse");
    let memory = MemoryHandle::new(1, Some(1)).expect("allocate one-page generated memory");
    let mut hosts = HostRegistry::new();
    hosts
        .register_memory("env", "mem", memory.clone())
        .expect("register generated imported memory");
    let mut instance =
        Instance::with_hosts(module, hosts).expect("generated imported-memory fixture must instantiate");

    let fixed_addresses = [0u32, 4, 8, 64, 1_024, 65_532];
    let mut rng = XorShift64::new(SEED ^ 0xa54f_f53a_5f1d_36f1);
    let mut model = HashMap::<u32, i32>::new();

    for step in 0..320 {
        let address = if step < fixed_addresses.len() {
            fixed_addresses[step]
        } else {
            ((rng.next_u64() % 16_384) as u32) * 4
        };
        let operation = rng.next_u64() % 3;

        match operation {
            0 => {
                let value = rng.next_i32();
                assert_eq!(
                    instance
                        .invoke_export("store", &[Value::I32(address as i32), Value::I32(value)])
                        .unwrap(),
                    None,
                    "guest store shape mismatch at seed={SEED:#018x} step={step}"
                );
                model.insert(address, value);
            }
            1 => {
                let value = rng.next_i32();
                memory
                    .write(address, &value.to_le_bytes())
                    .expect("generated host write must stay in bounds");
                model.insert(address, value);
                assert_eq!(
                    instance
                        .invoke_export("load", &[Value::I32(address as i32)])
                        .unwrap(),
                    Some(Value::I32(value)),
                    "guest did not observe host memory write at seed={SEED:#018x} step={step}"
                );
            }
            _ => {
                let expected = *model.get(&address).unwrap_or(&0);
                assert_eq!(
                    instance
                        .invoke_export("load", &[Value::I32(address as i32)])
                        .unwrap(),
                    Some(Value::I32(expected)),
                    "stateful load mismatch at seed={SEED:#018x} step={step} address={address}"
                );
            }
        }

        let expected = *model.get(&address).unwrap_or(&0);
        let bytes = memory
            .read(address, 4)
            .expect("generated host read must stay in bounds");
        assert_eq!(
            i32::from_le_bytes(bytes.try_into().expect("four-byte generated memory read")),
            expected,
            "host memory view diverged at seed={SEED:#018x} step={step} address={address}"
        );
    }
}
