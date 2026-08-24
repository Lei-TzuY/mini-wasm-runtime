use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};

const I32: u8 = 0x7f;
const I64: u8 = 0x7e;
const I64_GE_S: u8 = 0x59;
const I64_GE_U: u8 = 0x5a;

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

    let type_section = [0x01, 0x60, 0x02, I64, I64, 0x01, I32];
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

fn run(opcode: u8, lhs: i64, rhs: i64) -> i32 {
    let module = comparison_module(opcode);
    let mut instance = Instance::new(parse_module(&module).expect("parse i64 comparison fixture"))
        .expect("instantiate i64 comparison fixture");
    match instance
        .invoke_export("run", &[Value::I64(lhs), Value::I64(rhs)])
        .expect("execute i64 comparison")
    {
        Some(Value::I32(value)) => value,
        other => panic!("unexpected comparison result: {other:?}"),
    }
}

// Source-faithful boundary matrix from WebAssembly/spec test/core/i64.wast
// pinned at fc209c5ed8afc4dfeb9252024d217da3376c7a6f.
const GE_S_CASES: &[(i64, i64, i32)] = &[
    (0, 0, 1),
    (1, 1, 1),
    (-1, 1, 0),
    (i64::MIN, i64::MIN, 1),
    (i64::MAX, i64::MAX, 1),
    (-1, -1, 1),
    (1, 0, 1),
    (0, 1, 0),
    (i64::MIN, 0, 0),
    (0, i64::MIN, 1),
    (i64::MIN, -1, 0),
    (-1, i64::MIN, 1),
    (i64::MIN, i64::MAX, 0),
    (i64::MAX, i64::MIN, 1),
];

const GE_U_CASES: &[(i64, i64, i32)] = &[
    (0, 0, 1),
    (1, 1, 1),
    (-1, 1, 1),
    (i64::MIN, i64::MIN, 1),
    (i64::MAX, i64::MAX, 1),
    (-1, -1, 1),
    (1, 0, 1),
    (0, 1, 0),
    (i64::MIN, 0, 1),
    (0, i64::MIN, 0),
    (i64::MIN, -1, 0),
    (-1, i64::MIN, 1),
    (i64::MIN, i64::MAX, 1),
    (i64::MAX, i64::MIN, 0),
];

#[test]
fn i64_ge_s_matches_pinned_upstream_ordering_vectors() {
    for &(lhs, rhs, expected) in GE_S_CASES {
        assert_eq!(run(I64_GE_S, lhs, rhs), expected, "ge_s({lhs}, {rhs})");
    }
}

#[test]
fn i64_ge_u_matches_pinned_upstream_ordering_vectors() {
    for &(lhs, rhs, expected) in GE_U_CASES {
        assert_eq!(
            run(I64_GE_U, lhs, rhs),
            expected,
            "ge_u({lhs:#x}, {rhs:#x})"
        );
    }
}
