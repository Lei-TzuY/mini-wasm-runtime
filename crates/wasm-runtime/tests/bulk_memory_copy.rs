use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    assert!(payload.len() < 128);
    module.push(id);
    module.push(payload.len() as u8);
    module.extend(payload);
}

fn module_with_bodies(bodies: &[Vec<u8>], exports: &[(&str, u8)]) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x00, 0x01, 0x7f]);
    let mut functions = vec![bodies.len() as u8];
    functions.extend(std::iter::repeat(0x00).take(bodies.len()));
    push_section(&mut bytes, 3, &functions);
    push_section(&mut bytes, 5, &[0x01, 0x00, 0x01]);
    let mut export_payload = vec![exports.len() as u8];
    for (name, index) in exports {
        export_payload.push(name.len() as u8);
        export_payload.extend(name.as_bytes());
        export_payload.push(0x00);
        export_payload.push(*index);
    }
    push_section(&mut bytes, 7, &export_payload);
    let mut code = vec![bodies.len() as u8];
    for body in bodies {
        code.push((body.len() + 1) as u8);
        code.push(0x00);
        code.extend(body);
    }
    push_section(&mut bytes, 10, &code);
    bytes
}

fn store8(address: u8, value: u8, body: &mut Vec<u8>) {
    body.extend([0x41, address, 0x41, value, 0x3a, 0x00, 0x00]);
}

#[test]
fn memory_copy_uses_memmove_semantics_for_overlap() {
    let mut body = Vec::new();
    for (address, value) in [(0, 10), (1, 20), (2, 30), (3, 40), (4, 50), (5, 60)] {
        store8(address, value, &mut body);
    }
    body.extend([
        0x41, 0x02, 0x41, 0x00, 0x41, 0x06, 0xfc, 0x0a, 0x00, 0x00, 0x41, 0x07, 0x2d, 0x00, 0x00,
        0x0b,
    ]);
    let bytes = module_with_bodies(&[body], &[("run", 0)]);
    let mut instance = Instance::new(parse_module(&bytes).unwrap()).unwrap();
    assert_eq!(
        instance.invoke_export("run", &[]).unwrap(),
        Some(Value::I32(60))
    );
}

#[test]
fn memory_copy_preflights_destination_before_mutation() {
    let trap = vec![
        0x41, 0xff, 0xff, 0x03, 0x41, 0x00, 0x41, 0x02, 0xfc, 0x0a, 0x00, 0x00, 0x41, 0x00, 0x0b,
    ];
    let tail = vec![0x41, 0xff, 0xff, 0x03, 0x2d, 0x00, 0x00, 0x0b];
    let bytes = module_with_bodies(&[trap, tail], &[("trap", 0), ("tail", 1)]);
    let mut instance = Instance::new(parse_module(&bytes).unwrap()).unwrap();
    assert!(matches!(
        instance.invoke_export("trap", &[]),
        Err(RuntimeError::MemoryOutOfBounds { .. })
    ));
    assert_eq!(
        instance.invoke_export("tail", &[]).unwrap(),
        Some(Value::I32(0))
    );
}

#[test]
fn memory_copy_rejects_nonzero_memory_indices() {
    let body = vec![
        0x41, 0x00, 0x41, 0x00, 0x41, 0x00, 0xfc, 0x0a, 0x01, 0x00, 0x41, 0x00, 0x0b,
    ];
    let bytes = module_with_bodies(&[body], &[("run", 0)]);
    assert!(Instance::new(parse_module(&bytes).unwrap()).is_err());
}
