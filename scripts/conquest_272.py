from pathlib import Path

runtime = Path("crates/wasm-runtime/src/lib.rs")
text = runtime.read_text()
marker = """        256 => {\n            // Relaxed swizzle permits implementation-defined results for selectors 16..=127,"""
assert marker in text
arm = """        272 => {
            // f64x2.relaxed_max permits implementation-defined choice for NaN and
            // signed-zero ties. Reuse deterministic f64x2.max-compatible semantics.
            let rhs = numeric::v128_from_stack(stack)?;
            let lhs = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for lane in 0..2 {
                let start = lane * 8;
                let lhs_lane = f64::from_bits(u64::from_le_bytes(
                    lhs[start..start + 8].try_into().expect("f64x2 lane width"),
                ));
                let rhs_lane = f64::from_bits(u64::from_le_bytes(
                    rhs[start..start + 8].try_into().expect("f64x2 lane width"),
                ));
                let value = if lhs_lane.is_nan() || rhs_lane.is_nan() {
                    f64::NAN
                } else if lhs_lane == 0.0 && rhs_lane == 0.0 {
                    f64::from_bits(lhs_lane.to_bits() & rhs_lane.to_bits())
                } else if lhs_lane > rhs_lane {
                    lhs_lane
                } else {
                    rhs_lane
                };
                result[start..start + 8].copy_from_slice(&value.to_bits().to_le_bytes());
            }
            stack.push(Value::V128(Rc::new(result)));
        }
"""
text = text.replace(marker, arm + marker, 1)
assert "                    | 240..=271" in text
text = text.replace("                    | 240..=271", "                    | 240..=272", 1)
runtime.write_text(text)

typed = Path("crates/wasm-validator/src/typed.rs")
text = typed.read_text()
assert "                    269..=271 => {" in text
text = text.replace("                    269..=271 => {", "                    269..=272 => {", 1)
typed.write_text(text)

frontier_files = []
for path in Path("crates/wasm-runtime/tests").glob("*.rs"):
    text = path.read_text()
    if "subopcode: 272" not in text:
        continue
    path.write_text(text.replace("272", "273"))
    frontier_files.append(path)
assert len(frontier_files) >= 20, frontier_files

minmax = Path("crates/wasm-runtime/tests/simd_f64x2_minmax.rs")
text = minmax.read_text()
addition = r'''

#[test]
fn relaxed_f64x2_max_executes_ordered_lanes() {
    assert_eq!(
        lane_bits([3.0, -2.0], [4.0, -5.0], 272, 0),
        4.0f64.to_bits()
    );
    assert_eq!(
        lane_bits([3.0, -2.0], [4.0, -5.0], 272, 1),
        (-2.0f64).to_bits()
    );
}

#[test]
fn relaxed_f64x2_max_selects_positive_zero() {
    assert_eq!(
        lane_bits([0.0, -0.0], [-0.0, 0.0], 272, 0),
        0.0f64.to_bits()
    );
}

#[test]
fn validator_rejects_relaxed_f64x2_max_type_confusion() {
    let mut instructions = Vec::new();
    push_f64x2_const(&mut instructions, [1.0; 2]);
    push_i32_const(&mut instructions, 1);
    push_simd(&mut instructions, 272);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}
'''
assert "relaxed_f64x2_max_executes_ordered_lanes" not in text
minmax.write_text(text + addition)

source = Path("differential/tests/simd_relaxed_f64x2_min.rs").read_text()
diff = source.replace("relaxed_f64x2_min", "relaxed_f64x2_max")
diff = diff.replace("f64x2.relaxed_min", "f64x2.relaxed_max")
diff = diff.replace("relaxed min", "relaxed max")
diff = diff.replace("3.0f64.to_bits() as i64", "4.0f64.to_bits() as i64")
target = Path("differential/tests/simd_relaxed_f64x2_max.rs")
assert not target.exists()
target.write_text(diff)
