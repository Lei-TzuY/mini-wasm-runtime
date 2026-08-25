use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};

const I32: u8 = 0x7f;
const I64: u8 = 0x7e;
const I64_EQ: u8 = 0x51;
const I64_LT_S: u8 = 0x53;
const I64_LT_U: u8 = 0x54;
const I64_GT_S: u8 = 0x55;
const I64_GT_U: u8 = 0x56;
const I64_LE_S: u8 = 0x57;
const I64_LE_U: u8 = 0x58;
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

// Boundary-heavy deterministic corpus. These values deliberately cross the signed/unsigned
// interpretation boundary while retaining near-neighbor values on both sides of zero.
const VALUES: &[i64] = &[
    i64::MIN,
    i64::MIN + 1,
    -2,
    -1,
    0,
    1,
    2,
    i64::MAX - 1,
    i64::MAX,
];

#[test]
fn signed_i64_ordering_relations_hold_across_boundary_matrix() {
    for &lhs in VALUES {
        for &rhs in VALUES {
            let eq = run(I64_EQ, lhs, rhs);
            let lt = run(I64_LT_S, lhs, rhs);
            let gt = run(I64_GT_S, lhs, rhs);
            let le = run(I64_LE_S, lhs, rhs);
            let ge = run(I64_GE_S, lhs, rhs);

            assert_eq!(gt, run(I64_LT_S, rhs, lhs), "gt_s/lt_s duality");
            assert_eq!(ge, run(I64_LE_S, rhs, lhs), "ge_s/le_s duality");
            assert_eq!(gt, 1 - le, "gt_s must complement le_s");
            assert_eq!(ge, 1 - lt, "ge_s must complement lt_s");
            assert_eq!(lt + eq + gt, 1, "signed ordering must be trichotomous");
            assert_eq!(le, lt | eq, "le_s must equal lt_s OR eq");
            assert_eq!(ge, gt | eq, "ge_s must equal gt_s OR eq");
        }
    }
}

#[test]
fn unsigned_i64_ordering_relations_hold_across_boundary_matrix() {
    for &lhs in VALUES {
        for &rhs in VALUES {
            let eq = run(I64_EQ, lhs, rhs);
            let lt = run(I64_LT_U, lhs, rhs);
            let gt = run(I64_GT_U, lhs, rhs);
            let le = run(I64_LE_U, lhs, rhs);
            let ge = run(I64_GE_U, lhs, rhs);

            assert_eq!(gt, run(I64_LT_U, rhs, lhs), "gt_u/lt_u duality");
            assert_eq!(ge, run(I64_LE_U, rhs, lhs), "ge_u/le_u duality");
            assert_eq!(gt, 1 - le, "gt_u must complement le_u");
            assert_eq!(ge, 1 - lt, "ge_u must complement lt_u");
            assert_eq!(lt + eq + gt, 1, "unsigned ordering must be trichotomous");
            assert_eq!(le, lt | eq, "le_u must equal lt_u OR eq");
            assert_eq!(ge, gt | eq, "ge_u must equal gt_u OR eq");
        }
    }
}
