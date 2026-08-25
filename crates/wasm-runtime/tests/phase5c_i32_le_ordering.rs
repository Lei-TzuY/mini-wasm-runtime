use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};

const I32: u8 = 0x7f;
const I32_LE_S: u8 = 0x4c;
const I32_LE_U: u8 = 0x4d;

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

fn comparison_module(opcode: u8) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    let type_section = [0x01, 0x60, 0x02, I32, I32, 0x01, I32];
    push_section(&mut module, 1, &type_section);

    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);

    let body = [0x00, 0x20, 0x00, 0x20, 0x01, opcode, 0x0b];
    let mut code_section = vec![0x01];
    push_u32(&mut code_section, body.len() as u32);
    code_section.extend_from_slice(&body);
    push_section(&mut module, 10, &code_section);

    module
}

fn run(opcode: u8, lhs: i32, rhs: i32) -> i32 {
    let module = comparison_module(opcode);
    let mut instance = Instance::new(parse_module(&module).expect("parse i32 comparison fixture"))
        .expect("instantiate i32 comparison fixture");
    match instance
        .invoke_export("run", &[Value::I32(lhs), Value::I32(rhs)])
        .expect("execute i32 comparison")
    {
        Some(Value::I32(value)) => value,
        other => panic!("unexpected comparison result: {other:?}"),
    }
}

// Source-faithful boundary matrix from WebAssembly/spec test/core/i32.wast
// pinned at fc209c5ed8afc4dfeb9252024d217da3376c7a6f.
const LE_S_CASES: &[(i32, i32, i32)] = &[
    (0, 0, 1),
    (1, 1, 1),
    (-1, 1, 1),
    (i32::MIN, i32::MIN, 1),
    (i32::MAX, i32::MAX, 1),
    (-1, -1, 1),
    (1, 0, 0),
    (0, 1, 1),
    (i32::MIN, 0, 1),
    (0, i32::MIN, 0),
    (i32::MIN, -1, 1),
    (-1, i32::MIN, 0),
    (i32::MIN, i32::MAX, 1),
    (i32::MAX, i32::MIN, 0),
];

const LE_U_CASES: &[(i32, i32, i32)] = &[
    (0, 0, 1),
    (1, 1, 1),
    (-1, 1, 0),
    (i32::MIN, i32::MIN, 1),
    (i32::MAX, i32::MAX, 1),
    (-1, -1, 1),
    (1, 0, 0),
    (0, 1, 1),
    (i32::MIN, 0, 0),
    (0, i32::MIN, 1),
    (i32::MIN, -1, 1),
    (-1, i32::MIN, 0),
    (i32::MIN, i32::MAX, 0),
    (i32::MAX, i32::MIN, 1),
];

#[test]
fn i32_le_s_matches_pinned_upstream_ordering_vectors() {
    for &(lhs, rhs, expected) in LE_S_CASES {
        assert_eq!(run(I32_LE_S, lhs, rhs), expected, "le_s({lhs}, {rhs})");
    }
}

#[test]
fn i32_le_u_matches_pinned_upstream_ordering_vectors() {
    for &(lhs, rhs, expected) in LE_U_CASES {
        assert_eq!(
            run(I32_LE_U, lhs, rhs),
            expected,
            "le_u({lhs:#x}, {rhs:#x})"
        );
    }
}
