from pathlib import Path

p=Path('crates/wasm-runtime/src/lib.rs')
s=p.read_text()
old='''        240..=243 => {\n            let rhs = numeric::v128_from_stack(stack)?;'''
new='''        244..=245 => {\n            let rhs = numeric::v128_from_stack(stack)?;\n            let lhs = numeric::v128_from_stack(stack)?;\n            let mut result = [0u8; 16];\n            for lane in 0..2 {\n                let start = lane * 8;\n                let lhs_lane = f64::from_bits(u64::from_le_bytes(\n                    lhs[start..start + 8].try_into().expect("f64x2 lane width"),\n                ));\n                let rhs_lane = f64::from_bits(u64::from_le_bytes(\n                    rhs[start..start + 8].try_into().expect("f64x2 lane width"),\n                ));\n                let output = match subopcode {\n                    244 => {\n                        if lhs_lane.is_nan() || rhs_lane.is_nan() {\n                            f64::NAN\n                        } else if lhs_lane == 0.0 && rhs_lane == 0.0 {\n                            f64::from_bits(lhs_lane.to_bits() | rhs_lane.to_bits())\n                        } else if lhs_lane < rhs_lane { lhs_lane } else { rhs_lane }\n                    }\n                    245 => {\n                        if lhs_lane.is_nan() || rhs_lane.is_nan() {\n                            f64::NAN\n                        } else if lhs_lane == 0.0 && rhs_lane == 0.0 {\n                            f64::from_bits(lhs_lane.to_bits() & rhs_lane.to_bits())\n                        } else if lhs_lane > rhs_lane { lhs_lane } else { rhs_lane }\n                    }\n                    _ => unreachable!("matched f64x2 min max opcode"),\n                };\n                result[start..start + 8].copy_from_slice(&output.to_bits().to_le_bytes());\n            }\n            stack.push(Value::V128(Rc::new(result)));\n        }\n        240..=243 => {\n            let rhs = numeric::v128_from_stack(stack)?;'''
assert old in s
p.write_text(s.replace(old,new,1))

p=Path('crates/wasm-validator/src/typed.rs'); s=p.read_text(); old='''                    | 228..=235\n                    | 240..=243 => {'''; new='''                    | 228..=235\n                    | 240..=245 => {'''; assert old in s; p.write_text(s.replace(old,new,1))

p=Path('crates/wasm-runtime/src/lib.rs'); s=p.read_text(); old='''                    | 239\n                    | 240..=243\n                    | 142'''; new='''                    | 239\n                    | 240..=245\n                    | 142'''; assert old in s; p.write_text(s.replace(old,new,1))

# Move explicit adjacent fail-closed frontier assertions from f64x2.min to f64x2.pmin.
for p in Path('crates/wasm-runtime/tests').glob('simd_*.rs'):
    s=p.read_text()
    if '244' in s:
        p.write_text(s.replace('244', '246'))

Path('crates/wasm-runtime/tests/simd_f64x2_minmax.rs').write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::ValidationError;

fn push_u32(bytes: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 { byte |= 0x80; }
        bytes.push(byte);
        if value == 0 { break; }
    }
}

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn module(instructions: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0, 0, 0];
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x00, 0x01, 0x7e]);
    push_section(&mut bytes, 3, &[0x01, 0x00]);
    push_section(&mut bytes, 5, &[0x01, 0x00, 0x01]);
    push_section(&mut bytes, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);
    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend(body);
    push_section(&mut bytes, 10, &code);
    bytes
}

fn push_simd(i: &mut Vec<u8>, subopcode: u32) {
    i.push(0xfd);
    push_u32(i, subopcode);
}

fn push_f64x2_const(i: &mut Vec<u8>, lanes: [f64; 2]) {
    push_simd(i, 12);
    for lane in lanes { i.extend_from_slice(&lane.to_bits().to_le_bytes()); }
}

fn push_i32_const(i: &mut Vec<u8>, value: i32) {
    i.push(0x41);
    let mut value = value;
    loop {
        let byte = (value as u8) & 0x7f;
        value >>= 7;
        let sign = byte & 0x40 != 0;
        let done = (value == 0 && !sign) || (value == -1 && sign);
        i.push(if done { byte } else { byte | 0x80 });
        if done { break; }
    }
}

fn push_v128_store(i: &mut Vec<u8>) {
    push_simd(i, 11);
    i.extend_from_slice(&[4, 0]);
}

fn push_i64_load(i: &mut Vec<u8>, offset: u32) {
    i.push(0x29);
    i.push(3);
    push_u32(i, offset);
}

fn lane_bits(lhs: [f64; 2], rhs: [f64; 2], subopcode: u32, lane: u32) -> u64 {
    let mut instructions = Vec::new();
    instructions.extend_from_slice(&[0x02, 0x40]);
    push_f64x2_const(&mut instructions, lhs);
    push_f64x2_const(&mut instructions, rhs);
    push_simd(&mut instructions, subopcode);
    instructions.push(0x1a);
    instructions.push(0x0b);
    push_i32_const(&mut instructions, 0);
    push_f64x2_const(&mut instructions, lhs);
    push_f64x2_const(&mut instructions, rhs);
    push_simd(&mut instructions, subopcode);
    push_v128_store(&mut instructions);
    push_i32_const(&mut instructions, 0);
    push_i64_load(&mut instructions, lane * 8);

    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    let mut instance = Instance::new(parsed).expect("fixture validates");
    match instance.invoke_export_values("run", &[]).expect("fixture executes").as_slice() {
        [Value::I64(value)] => *value as u64,
        other => panic!("unexpected f64x2 minmax result: {other:?}"),
    }
}

#[test]
fn f64x2_min_max_are_lane_exact_and_preserve_signed_zero() {
    assert_eq!(lane_bits([3.0, 0.0], [2.0, -0.0], 244, 0), 2.0f64.to_bits());
    assert_eq!(lane_bits([3.0, 0.0], [2.0, -0.0], 244, 1), (-0.0f64).to_bits());
    assert_eq!(lane_bits([3.0, 0.0], [2.0, -0.0], 245, 0), 3.0f64.to_bits());
    assert_eq!(lane_bits([3.0, 0.0], [2.0, -0.0], 245, 1), 0.0f64.to_bits());
}

#[test]
fn f64x2_min_max_propagate_nan() {
    for op in [244, 245] {
        assert!(f64::from_bits(lane_bits([f64::NAN, 1.0], [2.0, f64::NAN], op, 0)).is_nan());
        assert!(f64::from_bits(lane_bits([f64::NAN, 1.0], [2.0, f64::NAN], op, 1)).is_nan());
    }
}

#[test]
fn validator_rejects_f64x2_minmax_type_confusion() {
    let mut instructions = Vec::new();
    push_f64x2_const(&mut instructions, [1.0; 2]);
    push_i32_const(&mut instructions, 1);
    push_simd(&mut instructions, 244);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(ValidationError::TypeMismatch { .. }))
    ));
}

#[test]
fn adjacent_f64x2_pmin_frontier_remains_fail_closed() {
    let mut instructions = Vec::new();
    push_f64x2_const(&mut instructions, [1.0, 2.0]);
    push_f64x2_const(&mut instructions, [3.0, 4.0]);
    push_simd(&mut instructions, 246);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(ValidationError::UnsupportedPrefixedOpcode {
            prefix: 0xfd,
            subopcode: 246,
            ..
        }))
    ));
}
''')

p=Path('docs/roadmap.md'); s=p.read_text(); old='''`f64x2.add`, `f64x2.sub`, `f64x2.mul`, and `f64x2.div` are executable with typed validation, structured-control handling, IEEE-754 edge regressions, and Wasmtime differential coverage; the adjacent `f64x2.min` opcode remains fail-closed.'''; new='''`f64x2.add`, `f64x2.sub`, `f64x2.mul`, and `f64x2.div` are executable with typed validation, structured-control handling, IEEE-754 edge regressions, and Wasmtime differential coverage; `f64x2.min` and `f64x2.max` are executable with ordered NaN propagation, signed-zero handling, typed validation, structured-control handling, and focused regressions; the adjacent `f64x2.pmin` opcode remains fail-closed.'''; assert old in s; p.write_text(s.replace(old,new,1))
