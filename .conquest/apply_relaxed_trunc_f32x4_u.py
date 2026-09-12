from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected one match in {path}: {old!r}, found {count}")
    path.write_text(text.replace(old, new, 1))


runtime = ROOT / "crates/wasm-runtime/src/lib.rs"
unsigned_runtime = '''        258 => {
            // i32x4.relaxed_trunc_f32x4_u permits a saturating result for
            // non-deterministic lanes. Rust's f32-to-u32 cast truncates and
            // saturates (NaN/negative -> 0), giving a valid deterministic lowering.
            let value = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for lane in 0..4 {
                let start = lane * 4;
                let input = f32::from_bits(u32::from_le_bytes(
                    value[start..start + 4]
                        .try_into()
                        .expect("f32x4 lane width"),
                ));
                result[start..start + 4].copy_from_slice(&(input as u32).to_le_bytes());
            }
            stack.push(Value::V128(Rc::new(result)));
        }
'''
replace_once(runtime, "        256 => {\n", unsigned_runtime + "        256 => {\n")
replace_once(runtime, "                    | 240..=257\n", "                    | 240..=258\n")

validator = ROOT / "crates/wasm-validator/src/typed.rs"
validator_text = validator.read_text()
anchor = '''                    257 => {
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }
'''
if validator_text.count(anchor) != 1:
    raise SystemExit(f"expected one typed-validator 257 anchor, found {validator_text.count(anchor)}")
validator.write_text(
    validator_text.replace(
        anchor,
        anchor
        + '''                    258 => {
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }
''',
        1,
    )
)

# Advance every legacy fail-closed SIMD frontier from 258 to 259 before adding
# the new 258-positive fixtures. At main@5fa93403, 258 is unsupported, so these
# helper calls/assertions are frontier sentinels rather than executable behavior.
for path in (ROOT / "crates/wasm-runtime/tests").glob("simd_*.rs"):
    text = path.read_text()
    text = re.sub(r"\b(simd|push_simd)\(([^\n]*?), 258\)", r"\1(\2, 259)", text)
    text = text.replace("subopcode: 258", "subopcode: 259")
    text = text.replace("258 frontier", "259 frontier")
    path.write_text(text)

signed_test = ROOT / "crates/wasm-runtime/tests/simd_relaxed_trunc_f32x4_s.rs"
unsigned_test = ROOT / "crates/wasm-runtime/tests/simd_relaxed_trunc_f32x4_u.rs"
text = signed_test.read_text()
text = text.replace("simd(&mut code, 257);", "simd(&mut code, 258);")
text = text.replace("simd(&mut code, 259);", "simd(&mut code, 259);")
text = text.replace("let lanes = [1.75, -2.75, 0.0, 12345.5];", "let lanes = [1.75, 2.75, 0.0, 12345.5];")
text = text.replace("assert_eq!(run_lane(lanes, 1), -2);", "assert_eq!(run_lane(lanes, 1), 2);")
text = text.replace("assert_eq!(run_lane(lanes, 1), i32::MAX);", "assert_eq!(run_lane(lanes, 1), -1);")
text = text.replace("assert_eq!(run_lane(lanes, 2), i32::MIN);", "assert_eq!(run_lane(lanes, 2), 0);")
text = text.replace("258 frontier fixture", "259 frontier fixture")
text = text.replace("subopcode: 258", "subopcode: 259")
unsigned_test.write_text(text)

signed_diff = ROOT / "differential/tests/simd_relaxed_trunc_f32x4_s.rs"
unsigned_diff = ROOT / "differential/tests/simd_relaxed_trunc_f32x4_u.rs"
text = signed_diff.read_text()
text = text.replace("i32x4.relaxed_trunc_f32x4_s", "i32x4.relaxed_trunc_f32x4_u")
text = text.replace("1.75 -2.75 0 12345.5", "1.75 2.75 0 12345.5")
text = text.replace("vec![1, -2, 12345]", "vec![1, 2, 12345]")
unsigned_diff.write_text(text)

roadmap = ROOT / "docs/roadmap.md"
roadmap_text = roadmap.read_text()
bullet = "- Relaxed SIMD `i32x4.relaxed_trunc_f32x4_u` (subopcode 258 / 0x102) is executable with a deterministic saturating lowering permitted by relaxed conversion semantics, typed `v128 -> v128` validation, focused runtime regressions, deterministic-lane Wasmtime differential evidence, and subopcode 259 retained as the fail-closed frontier.\n"
if "i32x4.relaxed_trunc_f32x4_u" not in roadmap_text:
    if not roadmap_text.endswith("\n"):
        roadmap_text += "\n"
    roadmap.write_text(roadmap_text + bullet)
