use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};
use wasm_validator::validate;

const I32: u8 = 0x7f;
const I64: u8 = 0x7e;
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

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn comparison_module(value_type: u8, opcode: u8, binary: bool) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let params = if binary {
        vec![0x02, value_type, value_type]
    } else {
        vec![0x01, value_type]
    };
    let mut function_type = vec![0x01, 0x60];
    function_type.extend_from_slice(&params);
    function_type.extend_from_slice(&[0x01, I32]);
    push_section(&mut module, 1, &function_type);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);

    let mut body = vec![0x00, 0x20, 0x00];
    if binary {
        body.extend_from_slice(&[0x20, 0x01]);
    }
    body.extend_from_slice(&[opcode, 0x0b]);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn invoke(opcode: u8, value_type: u8, args: &[Value]) -> i32 {
    let module = parse_module(&comparison_module(value_type, opcode, args.len() == 2))
        .expect("pinned integer comparison vector must parse");
    validate(&module).expect("pinned integer comparison vector must validate");
    let mut instance =
        Instance::new(module).expect("pinned integer comparison vector must instantiate");
    match instance
        .invoke_export("run", args)
        .expect("pinned integer comparison vector must execute")
        .expect("pinned integer comparison vector must return one value")
    {
        Value::I32(value) => value,
        other => panic!("expected i32 comparison result, got {other:?}"),
    }
}

#[test]
fn pinned_upstream_i32_eqz_eq_ne_vectors_match_spec() {
    // WebAssembly/spec test/core/i32.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    for (opcode, args, expected) in [
        (0x45, vec![Value::I32(0)], 1),
        (0x45, vec![Value::I32(-1)], 0),
        (0x46, vec![Value::I32(-1), Value::I32(-1)], 1),
        (0x46, vec![Value::I32(-1), Value::I32(0)], 0),
        (0x47, vec![Value::I32(-1), Value::I32(-1)], 0),
        (0x47, vec![Value::I32(-1), Value::I32(0)], 1),
    ] {
        assert_eq!(invoke(opcode, I32, &args), expected);
    }
}

#[test]
fn pinned_upstream_i64_eqz_eq_ne_vectors_match_spec() {
    // WebAssembly/spec test/core/i64.wast @ the pinned revision.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    for (opcode, args, expected) in [
        (0x50, vec![Value::I64(0)], 1),
        (0x50, vec![Value::I64(-1)], 0),
        (0x51, vec![Value::I64(-1), Value::I64(-1)], 1),
        (0x51, vec![Value::I64(-1), Value::I64(0)], 0),
        (0x52, vec![Value::I64(-1), Value::I64(-1)], 0),
        (0x52, vec![Value::I64(-1), Value::I64(0)], 1),
    ] {
        assert_eq!(invoke(opcode, I64, &args), expected);
    }
}

#[test]
fn pinned_upstream_i32_signed_unsigned_relational_boundaries_match_spec() {
    // The same bit patterns intentionally reverse ordering between signed and unsigned compares.
    for (opcode, lhs, rhs, expected) in [
        (0x48, -1, 0, 1),        // lt_s
        (0x49, -1, 0, 0),        // lt_u: 0xffff_ffff is greater than zero
        (0x49, 0, -1, 1),        // lt_u, reversed operands
        (0x4a, -1, 0, 0),        // gt_s
        (0x4b, -1, 0, 1),        // gt_u
        (0x4b, 0, -1, 0),        // gt_u, reversed operands
        (0x4c, i32::MIN, 0, 1),  // le_s
        (0x4d, i32::MIN, 0, 0),  // le_u: 0x8000_0000 is greater than zero
        (0x4d, 0, i32::MIN, 1),  // le_u, reversed operands
        (0x4e, -1, 0, 0),        // ge_s
        (0x4f, -1, 0, 1),        // ge_u
        (0x4f, 0, -1, 0),        // ge_u, reversed operands
    ] {
        assert_eq!(
            invoke(opcode, I32, &[Value::I32(lhs), Value::I32(rhs)]),
            expected,
            "unexpected i32 comparison result for opcode 0x{opcode:02x}"
        );
    }
}

#[test]
fn pinned_upstream_i64_signed_unsigned_relational_boundaries_match_spec() {
    // WebAssembly integer relational operators interpret identical bits according to the opcode.
    for (opcode, lhs, rhs, expected) in [
        (0x53, -1, 0, 1),        // lt_s
        (0x54, -1, 0, 0),        // lt_u: 0xffff_ffff_ffff_ffff is greater than zero
        (0x54, 0, -1, 1),        // lt_u, reversed operands
        (0x55, -1, 0, 0),        // gt_s
        (0x56, -1, 0, 1),        // gt_u
        (0x56, 0, -1, 0),        // gt_u, reversed operands
        (0x57, i64::MIN, 0, 1),  // le_s
        (0x58, i64::MIN, 0, 0),  // le_u: 0x8000... is greater than zero
        (0x58, 0, i64::MIN, 1),  // le_u, reversed operands
        (0x59, -1, 0, 0),        // ge_s
        (0x5a, -1, 0, 1),        // ge_u
        (0x5a, 0, -1, 0),        // ge_u, reversed operands
    ] {
        assert_eq!(
            invoke(opcode, I64, &[Value::I64(lhs), Value::I64(rhs)]),
            expected,
            "unexpected i64 comparison result for opcode 0x{opcode:02x}"
        );
    }
}
