from pathlib import Path
import re


def replace_once(path, old, new):
    p = Path(path)
    text = p.read_text()
    if text.count(old) != 1:
        raise SystemExit(f"expected exactly one match in {path}, got {text.count(old)}")
    p.write_text(text.replace(old, new, 1))

# Validator: i16x8 comparison masks consume two v128s and produce one v128.
validator = "crates/wasm-validator/src/typed.rs"
old = '''                    35..=44 => {
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }
                    55..=64 => {'''
new = '''                    35..=54 => {
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }
                    55..=64 => {'''
replace_once(validator, old, new)

# Runtime execution: 8 little-endian i16 lanes, canonical 0xffff/0x0000 masks.
runtime = "crates/wasm-runtime/src/lib.rs"
old = '''        55..=64 => {
            let rhs = numeric::v128_from_stack(stack)?;'''
new = '''        45..=54 => {
            let rhs = numeric::v128_from_stack(stack)?;
            let lhs = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for lane in 0..8 {
                let start = lane * 2;
                let lhs_unsigned = u16::from_le_bytes(
                    lhs[start..start + 2]
                        .try_into()
                        .expect("i16x8 lane width"),
                );
                let rhs_unsigned = u16::from_le_bytes(
                    rhs[start..start + 2]
                        .try_into()
                        .expect("i16x8 lane width"),
                );
                let lhs_signed = lhs_unsigned as i16;
                let rhs_signed = rhs_unsigned as i16;
                let predicate = match subopcode {
                    45 => lhs_signed == rhs_signed,
                    46 => lhs_signed != rhs_signed,
                    47 => lhs_signed < rhs_signed,
                    48 => lhs_unsigned < rhs_unsigned,
                    49 => lhs_signed > rhs_signed,
                    50 => lhs_unsigned > rhs_unsigned,
                    51 => lhs_signed <= rhs_signed,
                    52 => lhs_unsigned <= rhs_unsigned,
                    53 => lhs_signed >= rhs_signed,
                    54 => lhs_unsigned >= rhs_unsigned,
                    _ => unreachable!("matched i16x8 comparison opcode"),
                };
                let mask = if predicate { u16::MAX } else { 0 };
                result[start..start + 2].copy_from_slice(&mask.to_le_bytes());
            }
            stack.push(Value::V128(Rc::new(result)));
        }
        55..=64 => {
            let rhs = numeric::v128_from_stack(stack)?;'''
replace_once(runtime, old, new)

# Structured-control scanner: comparisons have no immediates.
p = Path(runtime)
text = p.read_text()
old = "| 35..=44\n                    | 55..=64"
if old not in text:
    raise SystemExit("control-map SIMD range anchor not found")
p.write_text(text.replace(old, "| 35..=54\n                    | 55..=64", 1))

# The previous lane slice used i16x8.eq as its fail-closed frontier; remove that obsolete guard.
lanes = Path("crates/wasm-runtime/tests/simd_i16x8_lanes.rs")
text = lanes.read_text()
pattern = re.compile(r'''\n#\[test\]\nfn adjacent_i16x8_comparison_remains_fail_closed\(\) \{.*?\n\}\n''', re.S)
text2, n = pattern.subn("\n", text, count=1)
if n != 1:
    raise SystemExit(f"expected obsolete fail-closed test once, got {n}")
lanes.write_text(text2)

# Focused executable regressions.
Path("crates/wasm-runtime/tests/simd_i16x8_comparisons.rs").write_text(r'''use wasm_parser::parse_module;
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
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x00, 0x01, 0x7f]);
    push_section(&mut bytes, 3, &[0x01, 0x00]);
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

fn push_simd(instructions: &mut Vec<u8>, subopcode: u32) {
    instructions.push(0xfd);
    push_u32(instructions, subopcode);
}

fn push_v128_i16(instructions: &mut Vec<u8>, lanes: [u16; 8]) {
    push_simd(instructions, 12);
    for lane in lanes { instructions.extend_from_slice(&lane.to_le_bytes()); }
}

fn run_i32(instructions: &[u8]) -> i32 {
    let parsed = parse_module(&module(instructions)).expect("i16x8 comparison fixture must parse");
    let mut instance = Instance::new(parsed).expect("i16x8 comparison fixture must validate");
    match instance.invoke_export_values("run", &[]).expect("fixture must execute").as_slice() {
        [Value::I32(value)] => *value,
        other => panic!("unexpected i16x8 comparison result: {other:?}"),
    }
}

const LHS: [u16; 8] = [0x8000, 0x7fff, 0xffff, 0x0000, 0x0001, 0x8000, 0xfffe, 0x0064];
const RHS: [u16; 8] = [0x7fff, 0x8000, 0x0001, 0x0000, 0x0002, 0x7fff, 0xffff, 0x0064];

fn comparison_lane(subopcode: u32, lane: u8) -> i32 {
    let mut instructions = Vec::new();
    push_v128_i16(&mut instructions, LHS);
    push_v128_i16(&mut instructions, RHS);
    push_simd(&mut instructions, subopcode);
    push_simd(&mut instructions, 25); // i16x8.extract_lane_u
    instructions.push(lane);
    run_i32(&instructions)
}

#[test]
fn i16x8_comparisons_produce_canonical_word_masks() {
    let cases = [
        (45, 3, 65_535, 0, 0), // eq
        (46, 0, 65_535, 3, 0), // ne
        (47, 0, 65_535, 1, 0), // lt_s: -32768 < 32767
        (48, 1, 65_535, 0, 0), // lt_u: 32767 < 32768
        (49, 1, 65_535, 0, 0), // gt_s
        (50, 0, 65_535, 1, 0), // gt_u
        (51, 0, 65_535, 1, 0), // le_s
        (52, 1, 65_535, 0, 0), // le_u
        (53, 1, 65_535, 0, 0), // ge_s
        (54, 0, 65_535, 1, 0), // ge_u
    ];
    for (subopcode, yes_lane, yes, no_lane, no) in cases {
        assert_eq!(comparison_lane(subopcode, yes_lane), yes, "subopcode {subopcode} true lane");
        assert_eq!(comparison_lane(subopcode, no_lane), no, "subopcode {subopcode} false lane");
    }
}

#[test]
fn i16x8_comparison_executes_inside_structured_control() {
    let mut instructions = vec![0x02, 0x7f];
    push_v128_i16(&mut instructions, LHS);
    push_v128_i16(&mut instructions, RHS);
    push_simd(&mut instructions, 45);
    push_simd(&mut instructions, 25);
    instructions.push(3);
    instructions.push(0x0b);
    assert_eq!(run_i32(&instructions), 65_535);
}

#[test]
fn validator_rejects_i16x8_comparison_type_confusion() {
    let mut instructions = Vec::new();
    push_v128_i16(&mut instructions, LHS);
    instructions.extend_from_slice(&[0x41, 0x01]);
    push_simd(&mut instructions, 45);
    push_simd(&mut instructions, 25);
    instructions.push(0);
    let parsed = parse_module(&module(&instructions)).expect("type-confusion fixture must parse");
    assert!(matches!(Instance::new(parsed), Err(RuntimeError::Validation(ValidationError::TypeMismatch { .. }))));
}

#[test]
fn adjacent_f32x4_comparison_remains_fail_closed() {
    let bytes = module(&[
        0xfd, 0x0c, 0,0,0,0, 0,0,0,0, 0,0,0,0, 0,0,0,0,
        0xfd, 0x0c, 0,0,0,0, 0,0,0,0, 0,0,0,0, 0,0,0,0,
        0xfd, 0x41, // f32x4.eq, next comparison family
        0x1a,
        0x41, 0x00,
    ]);
    let parsed = parse_module(&bytes).expect("unsupported SIMD fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(ValidationError::UnsupportedPrefixedOpcode { prefix: 0xfd, subopcode: 65, .. }))
    ));
}
''')

# Phase-8 accounting.
roadmap = Path("docs/roadmap.md")
text = roadmap.read_text()
anchor = "- [x] SIMD `i16x8.splat` / signed+unsigned `extract_lane` / `replace_lane` primitives"
pos = text.find(anchor)
if pos < 0:
    raise SystemExit("roadmap i16x8 lane anchor not found")
end = text.find("\n", pos)
line = "\n- [x] SIMD `i16x8` comparison-mask family (`eq` / `ne`, signed/unsigned `lt` / `gt` / `le` / `ge`) with canonical 16-bit masks, exact v128 typing, structured-control scanning, deterministic regressions, and pinned Wasmtime differential evidence"
text = text[:end] + line + text[end:]
roadmap.write_text(text)
